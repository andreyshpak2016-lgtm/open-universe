//! # ou-server
//!
//! Авторитетный сервер единого шарда: фиксированный tick loop (30 Гц),
//! приём ввода от игроков, шаг симуляции и рассылка дельта‑снимков с учётом
//! зон интереса (interest management).
//!
//! Здесь нет реальных сокетов — это **детерминированный каркас**: ввод и вывод
//! абстрагированы, поэтому ядро можно прогонять в тестах и симуляциях. Реальный
//! UDP‑транспорт подключается поверх (см. `Transport`).

use ou_build::{BuildError, Color as BuildColor, VoxelGrid};
use ou_combat::{weapon, ShipHull, Weapon};
use ou_protocol::{
    BuildAction, BuildCommand, BuildEdit, BuildReason, EntityState, InputFrame, Message, NetId,
    ShotEvent, Snapshot, Vec3,
};
use ou_script::ScriptEngine;
use ou_sim::World;
use ou_spatial::SpatialGrid;
use std::collections::{BTreeMap, BTreeSet};

/// Радиус зоны интереса: игрок видит сущности в этом радиусе (ед.).
pub const INTEREST_RADIUS: f64 = 2_000.0;

/// Размер ячейки пространственного индекса. Должен быть сопоставим с радиусом
/// интереса: слишком мелкая сетка даёт много пустых ячеек, слишком крупная —
/// лишние проверки. Половина радиуса — хороший компромисс.
pub const CELL_SIZE: f64 = INTEREST_RADIUS / 2.0;

/// Конструкция: воксельная сетка плюс права и ревизии (совместная стройка).
#[derive(Clone, Debug)]
pub struct Construction {
    pub grid: VoxelGrid,
    /// Кто владеет конструкцией (может разрешать стройку другим).
    pub owner: NetId,
    /// Кому владелец разрешил строить (кроме него самого).
    pub builders: BTreeSet<NetId>,
    /// Номер последней применённой правки (растёт монотонно).
    pub revision: u64,
    /// Для каждого клиента — последняя отправленная ему ревизия и очередь
    /// неприменённых правок (чтобы при потере пакета клиент их дочитал).
    pub pending: BTreeMap<NetId, (u64, Vec<BuildEdit>)>,
}

impl Construction {
    pub fn new(owner: NetId) -> Self {
        Construction {
            grid: VoxelGrid::new(),
            owner,
            builders: BTreeSet::new(),
            revision: 0,
            pending: BTreeMap::new(),
        }
    }

    /// Может ли игрок редактировать эту конструкцию.
    pub fn can_edit(&self, player: NetId) -> bool {
        player == self.owner || self.builders.contains(&player)
    }

    /// Разрешить игроку строить.
    pub fn grant(&mut self, player: NetId) {
        self.builders.insert(player);
    }
}

/// Подключённый игрок.
#[derive(Clone, Debug)]
pub struct Connection {
    pub player_id: NetId,
    /// Сущность, которой управляет игрок (может отличаться от player_id).
    pub entity_id: NetId,
    /// Последний кадр ввода от клиента.
    pub last_input: InputFrame,
    /// Последний подтверждённый кадр.
    pub acked_seq: u32,
    /// Сущности, которые этот клиент уже знает (для дельт).
    pub known: BTreeSet<NetId>,
    /// Последнее отправленное клиенту состояние каждой сущности.
    /// Позволяет слать только те сущности, что реально изменились.
    pub last_sent: BTreeMap<NetId, EntityState>,
}

/// Результат шага: что нужно отправить каждому клиенту.
#[derive(Debug, Default)]
pub struct SendPlan {
    pub messages: BTreeMap<NetId, Vec<Message>>,
}

pub struct Server {
    pub world: World,
    pub connections: BTreeMap<NetId, Connection>,
    /// Пространственный индекс: ускоряет interest management.
    pub grid: SpatialGrid,
    /// Движок скриптов игроков (автопилоты, турели, логистика).
    pub scripts: ScriptEngine,
    /// Сущность → имя скрипта, который ей управляет.
    pub scripted: BTreeMap<NetId, String>,
    /// Конструкции сущностей: что игроки построили (права + ревизии).
    pub constructions: BTreeMap<NetId, Construction>,
    /// Корпус каждой сущности: щит, броня, критические повреждения.
    pub hulls: BTreeMap<NetId, ShipHull>,
    /// Оружие каждой сущности (по умолчанию — автопушка).
    pub weapons: BTreeMap<NetId, Weapon>,
    /// Тик последнего выстрела сущности (для перезарядки).
    pub last_fire_tick: BTreeMap<NetId, u64>,
    /// Номер выстрела сущности (для зерна детерминированного попадания).
    pub shot_seq: BTreeMap<NetId, u64>,
    next_entity_id: NetId,
}

impl Server {
    pub fn new() -> Self {
        Server {
            world: World::new(),
            connections: BTreeMap::new(),
            grid: SpatialGrid::new(CELL_SIZE),
            scripts: ScriptEngine::new(),
            scripted: BTreeMap::new(),
            constructions: BTreeMap::new(),
            hulls: BTreeMap::new(),
            weapons: BTreeMap::new(),
            last_fire_tick: BTreeMap::new(),
            shot_seq: BTreeMap::new(),
            next_entity_id: 1,
        }
    }

    /// Создать пустую конструкцию для сущности.
    pub fn new_construction(&mut self, entity: NetId) {
        self.constructions.insert(entity, Construction::new(entity));
    }

    /// Применить команду строительства (ставить/удалить/покрасить).
    ///
    /// Возвращает результат, который сервер отправит клиенту. Мир не роняется
    /// при любой ошибке — возвращается причина.
    ///
    /// Проверяет **права**: редактировать может владелец или тот, кому он
    /// разрешил. Первая команда от игрока создаёт его конструкцию.
    pub fn apply_build_as(&mut self, author: NetId, cmd: &BuildCommand) -> BuildReason {
        // Права проверяем до создания конструкции.
        if let Some(c) = self.constructions.get(&cmd.entity) {
            if !c.can_edit(author) {
                return BuildReason::Invalid;
            }
        }
        let c = self
            .constructions
            .entry(cmd.entity)
            .or_insert_with(|| Construction::new(author));
        let color = BuildColor(cmd.color.0, cmd.color.1, cmd.color.2);
        let res: Result<(), BuildError> = match cmd.action {
            BuildAction::Place => c.grid.place(cmd.coord, color),
            BuildAction::Remove => c.grid.remove(cmd.coord).map(|_| ()),
            BuildAction::Paint => c.grid.paint(cmd.coord, color),
        };
        match res {
            Ok(()) => {
                // Записываем правку и готовим её к рассылке ВСЕМ, включая автора:
                // клиент рисует только то, что сервер авторитетно принял. Иначе
                // автор не увидел бы свой блок (а при отказе — нарисовал бы лишнее).
                c.revision += 1;
                let edit = BuildEdit {
                    entity: cmd.entity,
                    author,
                    action: cmd.action,
                    coord: cmd.coord,
                    color: ou_protocol::Color(cmd.color.0, cmd.color.1, cmd.color.2),
                    revision: c.revision,
                };
                for (_player, slot) in c.pending.iter_mut() {
                    slot.1.push(edit);
                }
                BuildReason::Ok
            }
            Err(BuildError::TooManyVoxels { .. }) => BuildReason::LimitReached,
            Err(BuildError::Disconnected(_)) => BuildReason::Disconnected,
            Err(_) => BuildReason::Invalid,
        }
    }

    /// Выдать игроку состояние правок для всех конструкций, которые он видит.
    ///
    /// Новому участнику сразу шлём **полный снимок** конструкции (все воксели),
    /// чтобы он увидел уже построенное, а не только будущие правки.
    fn sync_build_access(&mut self, player: NetId, visible: &BTreeSet<NetId>) {
        for entity in visible {
            let Some(c) = self.constructions.get_mut(entity) else {
                continue;
            };
            let can_edit = c.can_edit(player);
            let full = !c.pending.contains_key(&player);
            if full {
                // Полная выгрузка: каждая занятая ячейка — отдельная правка.
                let mut edits: Vec<BuildEdit> = Vec::with_capacity(c.grid.len());
                for (coord, bcolor) in c.grid.iter() {
                    edits.push(BuildEdit {
                        entity: *entity,
                        author: c.owner,
                        action: BuildAction::Place,
                        coord,
                        color: ou_protocol::Color(bcolor.0, bcolor.1, bcolor.2),
                        revision: c.revision,
                    });
                }
                let slot = c.pending.entry(player).or_insert((c.revision, Vec::new()));
                slot.1.extend(edits);
            }
            // Напоминаем права (клиент может их и не знать).
            let _ = can_edit;
        }
    }

    /// Владелец разрешает другому игроку строить в конструкции.
    pub fn grant_build(&mut self, owner: NetId, cmd_entity: NetId, player: NetId) -> bool {
        match self.constructions.get_mut(&cmd_entity) {
            Some(c) if c.owner == owner => {
                c.grant(player);
                true
            }
            _ => false,
        }
    }

    /// Подтверждение правок клиентом: чистим очередь до `revision`.
    pub fn ack_build(&mut self, player: NetId, entity: NetId, revision: u64) {
        if let Some(c) = self.constructions.get_mut(&entity) {
            if let Some(slot) = c.pending.get_mut(&player) {
                slot.1.retain(|e| e.revision > revision);
                slot.0 = revision;
            }
        }
    }

    /// Старый интерфейс (владелец = сама сущность) — для тестов и совместимости.
    pub fn apply_build(&mut self, cmd: &BuildCommand) -> BuildReason {
        self.apply_build_as(cmd.entity, cmd)
    }

    /// Обработать выстрел сущности `attacker` (авторитетно).
    ///
    /// Выбирает ближайшую цель в пределах дальности оружия, считает попадание
    /// детерминированно (зерно: тик, стрелок, номер выстрела) и наносит урон.
    /// Возвращает событие выстрела либо `None`, если стрелять нельзя (нет
    /// оружия, идёт перезарядка, нет цели).
    pub fn resolve_fire(&mut self, attacker: NetId, tick: u64) -> Option<ShotEvent> {
        let w = self.weapons.get(&attacker)?.clone();
        // Перезарядка.
        if let Some(last) = self.last_fire_tick.get(&attacker) {
            if tick.saturating_sub(*last) < w.cooldown {
                return None;
            }
        }
        let from = self.world.entities.get(&attacker)?.state.pos;

        // Ближайшая цель в пределах дальности (кроме себя и уже уничтоженных).
        let mut best: Option<(NetId, f64)> = None;
        for (id, ent) in self.world.entities.iter() {
            if *id == attacker {
                continue;
            }
            if self.hulls.get(id).map(|h| h.is_destroyed()).unwrap_or(true) {
                continue;
            }
            let d = from.distance(ent.state.pos);
            if d <= w.range {
                match best {
                    Some((_, bd)) if bd <= d => {}
                    _ => best = Some((*id, d)),
                }
            }
        }

        // Номер выстрела и зерно (детерминированно).
        let seq = self.shot_seq.entry(attacker).or_insert(0);
        *seq += 1;
        let seed = tick
            .wrapping_mul(0x9E3779B97F4A7C15)
            ^ (attacker as u64).wrapping_mul(0xBF58476D1CE4E5B9)
            ^ seq.wrapping_mul(0x94D049BB133111EB);
        self.last_fire_tick.insert(attacker, tick);

        let (target, dist) = match best {
            Some((id, d)) => (id, d),
            None => {
                // Стрельба в пустоту — всё равно событие (трассер без цели).
                return Some(ShotEvent {
                    attacker,
                    target: 0,
                    hit: false,
                    damage: 0,
                    destroyed: false,
                });
            }
        };

        let target_hull = *self.hulls.get(&target)?;
        let outcome = ou_combat::resolve_shot(&target_hull, &w, dist, seed);
        if let Some(h) = self.hulls.get_mut(&target) {
            ou_combat::apply_outcome(h, &outcome);
        }
        let destroyed = self.hulls.get(&target).map(|h| h.is_destroyed()).unwrap_or(false);
        let damage = outcome.shield_damage + outcome.armor_damage + outcome.core_damage;

        // Уничтоженную цель-дрон убираем из мира (у игроков есть соединение —
        // их не удаляем: игрок «возрождается» логикой выше).
        if destroyed && !self.connections.values().any(|c| c.entity_id == target) {
            self.despawn(target);
        }

        Some(ShotEvent {
            attacker,
            target,
            hit: outcome.hit,
            damage,
            destroyed,
        })
    }

    /// Назначить сущности скрипт (автопилот). С этого момента её ввод
    /// вычисляет скрипт, а не игрок.
    pub fn attach_script(&mut self, entity: NetId, script: &str, source: &str) -> Result<(), ou_script::Error> {
        self.scripts.compile(script, source)?;
        self.scripted.insert(entity, script.to_string());
        Ok(())
    }

    /// Подключение игрока: создаёт его сущность и точку интереса.
    pub fn connect(&mut self, player_id: NetId, spawn: Vec3) -> NetId {
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;
        self.world.spawn(entity_id, spawn);
        // Каждая сущность — корабль со щитом/бронёй и оружием по умолчанию.
        self.hulls.insert(entity_id, ShipHull::new(100, 200));
        if let Some(w) = weapon("автопушка") {
            self.weapons.insert(entity_id, w);
        }
        // У каждого игрока есть своя конструкция (её же он и редактирует).
        self.constructions.entry(entity_id).or_insert_with(|| Construction::new(player_id));
        self.connections.insert(
            player_id,
            Connection {
                player_id,
                entity_id,
                last_input: InputFrame::neutral(0),
                acked_seq: 0,
                known: BTreeSet::new(),
                last_sent: BTreeMap::new(),
            },
        );
        entity_id
    }

    /// Подключение игрока **на домашней планете** (точка появления на суше).
    ///
    /// Корабль привязывается к рельефу: работают гравитация и посадка.
    /// Используется реальным сервером (`ou-netd`).
    pub fn connect_on_planet(&mut self, player_id: NetId) -> NetId {
        // Небольшой зазор над поверхностью: гравитация мягко опустит корабль.
        let entity_id = self.connect(player_id, ou_sim::home_spawn(2.0));
        self.world.attach_terrain(entity_id, ou_sim::home_planet());
        entity_id
    }

    /// Создать цель-дрон (мишень без игрока).
    ///
    /// У дрона есть корпус (без щита, средняя броня), но нет соединения и
    /// оружия: он служит целью для проверки боя и виден всем рядом. Дрон
    /// «висит» в воздухе (гравитацию к нему не привязываем).
    pub fn spawn_target(&mut self, pos: Vec3) -> NetId {
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;
        self.world.spawn(entity_id, pos);
        self.hulls.insert(entity_id, ShipHull::new(0, 40));
        entity_id
    }

    /// Убрать сущность из мира (например, уничтоженную в бою).
    pub fn despawn(&mut self, entity: NetId) {
        self.world.entities.remove(&entity);
        self.hulls.remove(&entity);
        self.weapons.remove(&entity);
        self.last_fire_tick.remove(&entity);
        self.shot_seq.remove(&entity);
        self.constructions.remove(&entity);
        self.scripted.remove(&entity);
        self.grid.remove(entity);
    }

    /// Продолжить нумерацию сущностей после загрузки мира из снимка,
    /// чтобы новые сущности не столкнулись с уже существующими.
    pub fn adopt_world(&mut self, world: World) {
        let max_id = world.entities.keys().copied().max().unwrap_or(0);
        self.next_entity_id = max_id + 1;
        self.world = world;
        // Пересобрать пространственный индекс под загруженные позиции.
        for (id, ent) in self.world.entities.iter() {
            self.grid.update(*id, ent.state.pos);
        }
    }

    /// Текущий следующий идентификатор сущности (для диагностики).
    pub fn next_entity_id(&self) -> NetId {
        self.next_entity_id
    }

    /// Приём ввода от клиента (сервер хранит последний кадр).
    pub fn receive_input(&mut self, player_id: NetId, inputs: &[InputFrame]) {
        if let Some(conn) = self.connections.get_mut(&player_id) {
            // Применяем самый свежий кадр (по seq).
            if let Some(latest) = inputs.iter().max_by_key(|i| i.seq) {
                if latest.seq >= conn.last_input.seq {
                    conn.last_input = *latest;
                }
            }
        }
    }

    /// Один авторитетный тик: шаг мира + формирование рассылки.
    pub fn tick(&mut self) -> SendPlan {
        // 1. Собрать вводы по сущностям.
        let mut inputs: BTreeMap<NetId, InputFrame> = BTreeMap::new();
        for conn in self.connections.values() {
            inputs.insert(conn.entity_id, conn.last_input);
        }

        // 1a. Сущности со скриптами: ввод вычисляет автопилот.
        //     Порядок детерминирован (BTreeMap), время — из мира.
        let scripted: Vec<(NetId, String)> =
            self.scripted.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (entity, script) in scripted {
            let Some(ent) = self.world.entities.get(&entity) else {
                continue;
            };
            let state = ou_script::ScriptState {
                speed: ent.state.vel.length(),
                altitude: ent.state.pos.length(),
                heading: ent.state.yaw as f64,
                target_distance: 0.0,
            };
            let seq = self.world.tick as u32;
            match self.scripts.run_ship(&script, state, seq) {
                Ok(out) => {
                    inputs.insert(entity, out.input);
                }
                Err(e) => {
                    // Ошибка скрипта не должна ронять мир: логируем и продолжаем.
                    eprintln!("[скрипт '{script}'] {e}");
                }
            }
        }

        // 2. Шаг симуляции.
        self.world.step(&inputs);
        let tick = self.world.tick;

        // 3. Обновить пространственный индекс под новые позиции.
        for (id, ent) in self.world.entities.iter() {
            self.grid.update(*id, ent.state.pos);
        }

        // 3a. Бой: обработать намерение стрелять (fire) у всех, кто стрелял.
        //     Авторитетно: попадание и урон считает только сервер.
        let mut shots: Vec<ShotEvent> = Vec::new();
        let mut hull_dirty: BTreeSet<NetId> = BTreeSet::new();
        let shooters: Vec<NetId> = inputs
            .iter()
            .filter(|(_, i)| i.fire)
            .map(|(id, _)| *id)
            .collect();
        for id in shooters {
            if let Some(ev) = self.resolve_fire(id, tick) {
                hull_dirty.insert(id);
                if ev.target != 0 {
                    hull_dirty.insert(ev.target);
                }
                shots.push(ev);
            }
        }

        // 4. Для каждого клиента — интерес‑менеджмент и дельта.
        let mut plan = SendPlan::default();
        let all_states: BTreeMap<NetId, EntityState> = self.world.snapshot_map();

        // Фаза A: какие сущности видит каждый клиент (только чтение).
        let mut visible_by_player: BTreeMap<NetId, BTreeSet<NetId>> = BTreeMap::new();
        for (player_id, conn) in self.connections.iter() {
            let center = all_states
                .get(&conn.entity_id)
                .map(|e| e.pos)
                .unwrap_or(Vec3::ZERO);
            let mut visible: BTreeSet<NetId> =
                self.grid.query_radius(center, INTEREST_RADIUS).into_iter().collect();
            visible.insert(conn.entity_id);
            visible_by_player.insert(*player_id, visible);
        }

        // Фаза B: репликация правок конструкций (совместное строительство).
        let mut build_msgs: BTreeMap<NetId, Vec<Message>> = BTreeMap::new();
        for (player_id, visible) in &visible_by_player {
            self.sync_build_access(*player_id, visible);
        }
        for entity in self.constructions.keys().copied().collect::<Vec<_>>() {
            let c = self.constructions.get_mut(&entity).unwrap();
            let mut drained: Vec<(NetId, Vec<BuildEdit>)> = Vec::new();
            for (player, slot) in c.pending.iter_mut() {
                if !slot.1.is_empty() {
                    drained.push((*player, std::mem::take(&mut slot.1)));
                }
            }
            for (player, edits) in drained {
                // Права + сами правки.
                let can_edit = c.can_edit(player);
                let entry = build_msgs.entry(player).or_default();
                entry.push(Message::ServerBuildAccess { entity, can_edit });
                for e in edits {
                    entry.push(Message::ServerBuildEdit(e));
                }
            }
        }

        for (player_id, conn) in self.connections.iter_mut() {
            let visible = visible_by_player[player_id].clone();

            let baseline = conn.known.is_empty();
            let mut changed: Vec<EntityState> = Vec::new();
            for id in &visible {
                let state = all_states[id];
                let need = match conn.last_sent.get(id) {
                    // Новая сущность для клиента или состояние изменилось.
                    None => true,
                    Some(prev) => !states_equal(prev, &state),
                };
                if baseline || need {
                    changed.push(state);
                }
            }

            // Подтверждение ввода.
            conn.acked_seq = conn.last_input.seq;

            let snap = Snapshot {
                tick,
                ack_input_seq: conn.acked_seq,
                baseline,
                entities: changed,
            };
            let mut msgs = vec![Message::ServerSnapshot(snap)];
            msgs.push(Message::ServerAck {
                last_input_seq: conn.acked_seq,
                server_tick: tick,
            });
            // Правки конструкций — после снимка.
            if let Some(extra) = build_msgs.remove(player_id) {
                msgs.extend(extra);
            }
            // События боя: выстрелы и состояние корпуса — тем, кто их видит.
            // Видимость выстрела определяем по тому, видит ли клиент стрелка.
            for ev in &shots {
                let sees_attacker = visible.contains(&ev.attacker)
                    || conn.entity_id == ev.attacker;
                let sees_target = ev.target == 0
                    || visible.contains(&ev.target)
                    || conn.entity_id == ev.target;
                if sees_attacker && sees_target {
                    msgs.push(Message::ServerShot(*ev));
                }
            }
            for e in &hull_dirty {
                if visible.contains(e) || conn.entity_id == *e {
                    if let Some(h) = self.hulls.get(e) {
                        msgs.push(Message::ServerHull {
                            entity: *e,
                            shield: h.shield,
                            armor: h.armor,
                            destroyed: h.is_destroyed(),
                        });
                    }
                }
            }

            // Обновляем «что клиент знает и в каком виде».
            conn.last_sent = visible.iter().map(|id| (*id, all_states[id])).collect();
            conn.known = visible;
            plan.messages.insert(*player_id, msgs);
        }

        plan
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

/// Сравнение состояний на уровне точности провода.
fn states_equal(a: &EntityState, b: &EntityState) -> bool {
    a.pos.to_wire() == b.pos.to_wire()
        && a.vel.to_wire() == b.vel.to_wire()
        && a.yaw.to_bits() == b.yaw.to_bits()
        && a.pitch.to_bits() == b.pitch.to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_players_see_each_other_when_close() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(10.0, 0.0, 0.0));
        let plan = s.tick();
        let m1 = &plan.messages[&1];
        let snap = m1.iter().find_map(|m| match m {
            Message::ServerSnapshot(s) => Some(s),
            _ => None,
        }).unwrap();
        // Оба игрока рядом → игрок 1 видит 2 сущности.
        assert_eq!(snap.entities.len(), 2);
    }

    #[test]
    fn interest_management_hides_far_entities() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(INTEREST_RADIUS * 5.0, 0.0, 0.0));
        let plan = s.tick();
        let m1 = &plan.messages[&1];
        let snap = m1.iter().find_map(|m| match m {
            Message::ServerSnapshot(s) => Some(s),
            _ => None,
        }).unwrap();
        // Далеко → игрок 1 видит только себя.
        assert_eq!(snap.entities.len(), 1);
        assert_eq!(snap.entities[0].id, 1);
    }

    #[test]
    fn delta_snapshot_sends_only_new_entities() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        let first = s.tick();
        let s1 = first.messages[&1].iter().find_map(|m| match m {
            Message::ServerSnapshot(s) => Some(s.clone()),
            _ => None,
        }).unwrap();
        assert!(s1.baseline);
        assert_eq!(s1.entities.len(), 1);

        // Второй тик без ввода: состояние не меняется → дельта пустая.
        let second = s.tick();
        let s2 = second.messages[&1].iter().find_map(|m| match m {
            Message::ServerSnapshot(s) => Some(s.clone()),
            _ => None,
        }).unwrap();
        assert!(!s2.baseline);
        assert_eq!(s2.entities.len(), 0);
    }

    #[test]
    fn delta_snapshot_replicates_movement() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(50.0, 0.0, 0.0));
        // Базовая линия.
        s.tick();
        // Игрок 2 начинает двигаться.
        for tick in 1..=5u32 {
            s.receive_input(2, &[InputFrame { seq: tick, throttle: 1.0, yaw: -1.5707963, pitch: 0.0, fire: false }]);
        }
        let plan = s.tick();
        let snap = plan.messages[&1].iter().find_map(|m| match m {
            Message::ServerSnapshot(s) => Some(s.clone()),
            _ => None,
        }).unwrap();
        // Движущаяся сущность 2 должна попасть в дельту.
        assert!(snap.entities.iter().any(|e| e.id == 2), "движение не реплицировалось");
    }

    #[test]
    fn scripted_entity_flies_by_autopilot() {
        let mut s = Server::new();
        let e = s.connect(1, Vec3::ZERO);
        // Автопилот всегда даёт полную тягу.
        s.attach_script(e, "auto", "thrust(1)").unwrap();
        for _ in 0..30 {
            s.tick();
        }
        let ent = s.world.entities.get(&e).unwrap();
        assert!(ent.state.pos.length() > 1.0, "автопилот должен разогнать корабль");
    }

    #[test]
    fn scripted_entity_without_input_still_moves() {
        // Игрок не отправляет ввод — движение обеспечивает скрипт.
        let mut s = Server::new();
        let e = s.connect(1, Vec3::ZERO);
        s.attach_script(e, "auto", "thrust(1)").unwrap();
        let plan = s.tick();
        assert!(plan.messages.contains_key(&1));
        let ent = s.world.entities.get(&e).unwrap();
        assert!(ent.state.vel.length() > 0.0, "скрипт должен задать скорость");
    }

    #[test]
    fn broken_script_does_not_crash_world() {
        let mut s = Server::new();
        let e = s.connect(1, Vec3::ZERO);
        // Скрипт с ошибкой выполнения: деление на ноль.
        s.attach_script(e, "bad", "local x = 1 / 0").unwrap();
        // Мир должен пережить тик без паники.
        let plan = s.tick();
        assert!(plan.messages.contains_key(&1));
    }

    // --- редактор конструкций ---

    fn cmd(action: BuildAction, coord: (i32, i32, i32)) -> BuildCommand {
        BuildCommand { entity: 1, action, coord, color: ou_protocol::Color(200, 200, 200) }
    }

    #[test]
    fn build_places_adjacent_voxel() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        let r = s.apply_build(&cmd(BuildAction::Place, (1, 0, 0)));
        assert_eq!(r, BuildReason::Ok);
        let g = s.constructions.get(&1).unwrap();
        assert_eq!(g.grid.len(), 2, "ядро + новый воксель");
    }

    #[test]
    fn build_rejects_floating_voxel() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        // Далеко от ядра — нельзя.
        let r = s.apply_build(&cmd(BuildAction::Place, (20, 0, 0)));
        assert_eq!(r, BuildReason::Disconnected);
    }

    #[test]
    fn build_paint_and_remove_work() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.apply_build(&cmd(BuildAction::Place, (1, 0, 0)));
        assert_eq!(s.apply_build(&cmd(BuildAction::Paint, (1, 0, 0))), BuildReason::Ok);
        assert_eq!(s.apply_build(&cmd(BuildAction::Remove, (1, 0, 0))), BuildReason::Ok);
        // Повторное удаление — ошибка.
        assert_eq!(s.apply_build(&cmd(BuildAction::Remove, (1, 0, 0))), BuildReason::Invalid);
    }

    #[test]
    fn build_cannot_remove_core() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        assert_eq!(s.apply_build(&cmd(BuildAction::Remove, (0, 0, 0))), BuildReason::Invalid);
    }

    #[test]
    fn constructions_are_independent() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(100.0, 0.0, 0.0));
        // Игрок 1 ставит воксель.
        s.apply_build(&cmd(BuildAction::Place, (1, 0, 0)));
        // У игрока 2 конструкция своя.
        let r2 = s.apply_build(&BuildCommand {
            entity: 2,
            action: BuildAction::Remove,
            coord: (1, 0, 0),
            color: ou_protocol::Color(0, 0, 0),
        });
        assert_eq!(r2, BuildReason::Invalid, "у второго игрока этой ячейки нет");
        assert_eq!(s.constructions.get(&1).unwrap().grid.len(), 2);
    }

    // --- совместное строительство (multiplayer edit) ---

    /// Найти правки конструкций в сообщениях клиента.
    fn build_edits(msgs: &[Message]) -> Vec<ou_protocol::BuildEdit> {
        msgs.iter()
            .filter_map(|m| match m {
                Message::ServerBuildEdit(e) => Some(*e),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn build_edit_is_replicated_to_other_players() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(10.0, 0.0, 0.0));
        s.tick(); // оба узнали мир

        // Игрок 1 строит в своей конструкции.
        let reason = s.apply_build_as(1, &BuildCommand {
            entity: 1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(10, 20, 30),
        });
        assert_eq!(reason, BuildReason::Ok);

        // Следующий тик: правка должна быть отправлена НЕ только автору.
        let plan = s.tick();
        let edits = build_edits(&plan.messages[&2]);
        assert!(
            edits.iter().any(|e| e.entity == 1 && e.coord == (1, 0, 0) && e.author == 1),
            "правка игрока 1 должна прийти игроку 2"
        );
    }

    #[test]
    fn only_owner_or_granted_can_edit() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(10.0, 0.0, 0.0));

        // Игрок 2 пытается строить в конструкции игрока 1 — отказано.
        let bad = s.apply_build_as(2, &BuildCommand {
            entity: 1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(1, 1, 1),
        });
        assert_eq!(bad, BuildReason::Invalid, "чужой строить не может");

        // Владелец (игрок 1) разрешает игроку 2.
        assert!(s.grant_build(1, 1, 2));
        let ok = s.apply_build_as(2, &BuildCommand {
            entity: 1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(1, 1, 1),
        });
        assert_eq!(ok, BuildReason::Ok, "после разрешения строить можно");
    }

    #[test]
    fn new_player_receives_existing_construction() {
        let mut s = Server::new();
        let e1 = s.connect(1, Vec3::ZERO);
        // Игрок 1 уже что-то построил.
        s.apply_build_as(1, &BuildCommand {
            entity: e1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(5, 5, 5),
        });
        // Подключается игрок 2 рядом.
        s.connect(2, Vec3::new(10.0, 0.0, 0.0));
        let plan = s.tick();
        // Второй игрок должен получить полный снимок конструкции 1 (все воксели).
        let edits = build_edits(&plan.messages[&2]);
        let has_core = edits.iter().any(|e| e.coord == (0, 0, 0));
        let has_built = edits.iter().any(|e| e.coord == (1, 0, 0));
        assert!(has_core && has_built, "новый игрок видит уже построенное");
    }

    #[test]
    fn ack_stops_resending_edits() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(10.0, 0.0, 0.0));
        s.tick();
        s.apply_build_as(1, &BuildCommand {
            entity: 1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(1, 1, 1),
        });
        let plan = s.tick();
        let rev = build_edits(&plan.messages[&2]).iter().map(|e| e.revision).max().unwrap();
        // Игрок 2 подтверждает: повторно слать не нужно.
        s.ack_build(2, 1, rev);
        let plan2 = s.tick();
        assert!(build_edits(&plan2.messages[&2]).is_empty(), "после ack повторной рассылки нет");
    }

    #[test]
    fn remote_edit_is_authoritative_for_other_players() {
        // Правка приходит с номером ревизии, который растёт монотонно.
        let mut s = Server::new();
        let e1 = s.connect(1, Vec3::ZERO);
        for i in 1..=3 {
            s.apply_build_as(1, &BuildCommand {
                entity: e1,
                action: BuildAction::Place,
                coord: (i, 0, 0),
                color: ou_protocol::Color(1, 1, 1),
            });
        }
        assert_eq!(s.constructions[&e1].revision, 3, "ревизия растёт по числу правок");
    }

    // --- бой (авторитетная стрельба) ---

    #[test]
    fn fire_damages_nearest_target() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        let b = s.connect(2, Vec3::new(100.0, 0.0, 0.0));
        let hull_before = s.hulls[&b];
        // Игрок 1 стреляет.
        let ev = s.resolve_fire(a, 1).expect("выстрел должен состояться");
        assert_eq!(ev.attacker, a);
        assert_eq!(ev.target, b, "цель — ближайшая сущность");
        // Урон прошёл по цели (щит или броня уменьшились).
        let hull_after = s.hulls[&b];
        assert!(
            hull_after.shield < hull_before.shield || hull_after.armor < hull_before.armor,
            "попадание должно нанести урон"
        );
    }

    #[test]
    fn fire_respects_cooldown() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(100.0, 0.0, 0.0));
        assert!(s.resolve_fire(a, 1).is_some(), "первый выстрел — ок");
        // Второй выстрел в тот же тик — на перезарядке.
        assert!(s.resolve_fire(a, 1).is_none(), "перезарядка не даёт стрелять сразу");
    }

    #[test]
    fn fire_beyond_range_hits_nobody() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        // Далеко за пределами автопушки (1500 ед.).
        s.connect(2, Vec3::new(50_000.0, 0.0, 0.0));
        let ev = s.resolve_fire(a, 1).expect("выстрел в пустоту — тоже событие");
        assert_eq!(ev.target, 0, "вне дальности цели нет");
        assert!(!ev.hit);
    }

    #[test]
    fn fire_is_deterministic_for_same_tick() {
        let run = || {
            let mut s = Server::new();
            let a = s.connect(1, Vec3::ZERO);
            s.connect(2, Vec3::new(300.0, 0.0, 0.0));
            let e1 = s.resolve_fire(a, 5);
            let e2 = s.resolve_fire(a, 5); // на перезарядке — None
            (e1, e2, s.hulls[&2])
        };
        assert_eq!(run(), run(), "бои должен быть детерминированным");
    }

    #[test]
    fn fire_broadcasts_shot_to_nearby_player() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(100.0, 0.0, 0.0));
        s.tick(); // базовый снимок
        // Игрок 1 стреляет: ввод с fire.
        s.receive_input(1, &[InputFrame { seq: 1, throttle: 0.0, yaw: 0.0, pitch: 0.0, fire: true }]);
        let plan = s.tick();
        let msgs = &plan.messages[&2];
        assert!(
            msgs.iter().any(|m| matches!(m, Message::ServerShot(_))),
            "игрок 2 должен увидеть событие выстрела"
        );
    }

    #[test]
    fn drone_takes_damage_and_is_removed_when_destroyed() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        let drone = s.spawn_target(Vec3::new(60.0, 0.0, 0.0));
        assert!(s.hulls.contains_key(&drone));
        // Бьём дрона, пока не уничтожим (перезарядка 4 тика).
        let mut destroyed = false;
        for tick in (1..200u64).step_by(5) {
            if let Some(ev) = s.resolve_fire(a, tick) {
                if ev.target == drone && ev.destroyed {
                    destroyed = true;
                    break;
                }
            }
        }
        assert!(destroyed, "дрон должен быть уничтожен");
        assert!(
            !s.world.entities.contains_key(&drone),
            "уничтоженный дрон убирается из мира"
        );
    }
}
