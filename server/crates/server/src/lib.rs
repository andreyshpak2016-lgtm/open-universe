//! # ou-server
//!
//! Авторитетный сервер единого шарда: фиксированный tick loop (30 Гц),
//! приём ввода от игроков, шаг симуляции и рассылка дельта‑снимков с учётом
//! зон интереса (interest management).
//!
//! Здесь нет реальных сокетов — это **детерминированный каркас**: ввод и вывод
//! абстрагированы, поэтому ядро можно прогонять в тестах и симуляциях. Реальный
//! UDP‑транспорт подключается поверх (см. `Transport`).

use ou_protocol::{EntityState, InputFrame, Message, NetId, Snapshot, Vec3};
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
            next_entity_id: 1,
        }
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

        // 4. Для каждого клиента — интерес‑менеджмент и дельта.
        let mut plan = SendPlan::default();
        let all_states: BTreeMap<NetId, EntityState> = self.world.snapshot_map();

        for (player_id, conn) in self.connections.iter_mut() {
            let center = all_states
                .get(&conn.entity_id)
                .map(|e| e.pos)
                .unwrap_or(Vec3::ZERO);

            // Зона интереса через пространственный индекс: обходим только
            // близкие ячейки, а не весь мир. Своя сущность — всегда видна.
            let mut visible: BTreeSet<NetId> =
                self.grid.query_radius(center, INTEREST_RADIUS).into_iter().collect();
            visible.insert(conn.entity_id);

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
}
