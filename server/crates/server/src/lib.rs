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
use ou_economy::{Inventory, ItemRegistry, Market, MatchingEngine, Production, Side};
use ou_protocol::{
    BuildAction, BuildCommand, BuildEdit, BuildReason, EntityState, InputFrame, Message, NetId,
    ShotEvent, Snapshot, Vec3,
};
use ou_script::ScriptEngine;
use ou_sim::{Dent, Terrain, World};
use ou_spatial::SpatialGrid;
use std::collections::{BTreeMap, BTreeSet};

/// Радиус зоны интереса: игрок видит сущности в этом радиусе (ед.).
pub const INTEREST_RADIUS: f64 = 2_000.0;

/// Размер ячейки пространственного индекса. Должен быть сопоставим с радиусом
/// интереса: слишком мелкая сетка даёт много пустых ячеек, слишком крупная —
/// лишние проверки. Половина радиуса — хороший компромисс.
pub const CELL_SIZE: f64 = INTEREST_RADIUS / 2.0;

/// Через сколько тиков после уничтожения игрок возрождается.
pub const RESPAWN_TICKS: u64 = 120;   // 4 секунды при 30 Гц
/// Сколько тиков после возрождения игрок неуязвим.
pub const RESPAWN_INVULN_TICKS: u64 = 90;   // 3 секунды

// --- Взрывы боя: крупная воронка в рельефе при уничтожении у поверхности ---
/// Радиус кратера от взрыва (серверные единицы; 25 м).
pub const EXPLOSION_RADIUS: f64 = 25.0;
/// Глубина кратера от взрыва (серверные единицы; 12 м).
pub const EXPLOSION_DEPTH: f64 = 12.0;
/// Насколько низко над землёй должен быть взрыв (иначе в космосе рельефа нет).
pub const EXPLOSION_ALT_LIMIT: f64 = 120.0;
/// Первый слот воронок, отданный взрывам (копание занимает 0..63).
pub const EXPLOSION_SLOT_BASE: u32 = 40;
/// Сколько слотов отведено под взрывы (циклически переиспользуются).
pub const EXPLOSION_SLOTS: u32 = 24;

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
    /// Тик, на котором мёртвый игрок возродится (0 — жив).
    pub respawn_tick: u64,
    /// До какого тика игрок неуязвим после возрождения.
    pub invuln_until: u64,
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
    /// Учебные цели: точка спавна и тик возрождения (для полигона).
    pub target_spawns: BTreeMap<NetId, Vec3>,
    /// Тик, на котором дрон-цель возродится (0 — жив или удалён без возврата).
    pub target_respawn_tick: BTreeMap<NetId, u64>,
    /// Поза пешего персонажа каждой сущности: (позиция, курс, идёт ли).
    /// `None` в значении-словаре означает «в корабле» (поза снята).
    pub characters: BTreeMap<NetId, CharacterPose>,
    /// Авторитетный рельеф домашней планеты — один на весь мир.
    ///
    /// Копание меняет именно его (список воронок), поэтому физика всех
    /// сущностей и рассылка правок идут из одного источника.
    pub planet: Terrain,
    /// Экономика в игре: реестр предметов, кредиты и рынок (движок сделок).
    pub economy: Economy,
    /// Экономика изменилась с прошлого тика (надо разослать состояние рынка).
    pub market_dirty: bool,
    /// Воронки от взрывов этого тика: (слот, воронка) — для рассылки как dig.
    pub pending_explosions: Vec<(u32, Dent)>,
    /// Счётчик взрывов (выбор слота воронки, чтобы ямы не затирали друг друга).
    pub explosion_seq: u32,
    next_entity_id: NetId,
}

/// Экономическое состояние мира: предметы, кредиты игроков и рынок.
///
/// Живёт на сервере и меняется только авторитетно (копание даёт руду,
/// выставление ордера исполняется против стакана). Детерминировано: только
/// целые числа.
pub struct Economy {
    pub registry: ItemRegistry,
    /// Движок рынка: стакан ордеров + инвентарь (по владельцу и системе).
    pub engine: MatchingEngine,
    /// Каталог рецептов производства (руда → металл → сплав → детали → корабль).
    pub production: Production,
    /// Кредиты каждого игрока.
    pub credits: BTreeMap<NetId, u64>,
    /// Звёздная система домашней планеты (рынки изолированы по системам).
    pub system: u32,
    /// Id сырья, которое даёт копание (руда).
    pub ore_item: u32,
    /// Id топлива (покупается у рынка/тратится).
    pub fuel_item: u32,
    /// Id предметов по «полосам глубины»: лёд, кристалл, металл.
    pub ice_item: u32,
    pub crystal_item: u32,
    pub metal_item: u32,
}

impl Economy {
    /// Новая экономика с рынком по умолчанию и стартовым капиталом игроков.
    fn new() -> Self {
        let registry = ItemRegistry::default_universe();
        let find = |n: &str| -> u32 { (1..=registry.len() as u32).find(|i| registry.name(*i) == n).unwrap_or(0) };
        let engine = MatchingEngine::new(Market::new(), Inventory::new());
        let production = Production::default_chain(&registry);
        Economy {
            ore_item: find("руда"),
            fuel_item: find("топливо"),
            ice_item: find("лёд"),
            crystal_item: find("кристалл"),
            metal_item: find("металл"),
            registry,
            engine,
            production,
            credits: BTreeMap::new(),
            system: 1,
        }
    }

    /// Что даёт копание на глубине `depth_m` (метры от поверхности).
    ///
    /// Полосы детерминированы: у поверхности — руда, глубже — лёд, кристалл и
    /// металл. Чем глубже, тем ценнее и реже добыча. Возвращает (id, количество).
    pub fn ore_at_depth(&self, depth_m: f64) -> (u32, u64) {
        let d = depth_m.max(0.0);
        if d < 15.0 {
            (self.ore_item, 3)       // руда, близко к поверхности
        } else if d < 40.0 {
            (self.ice_item, 2)       // лёд
        } else if d < 80.0 {
            (self.crystal_item, 1)   // кристалл
        } else {
            (self.metal_item, 1)     // металл, глубоко
        }
    }

    /// Слой (по глубине) для отрисовки: 0 руда, 1 лёд, 2 кристалл, 3 металл.
    pub fn ore_layer(depth_m: f64) -> u8 {
        let d = depth_m.max(0.0);
        if d < 15.0 { 0 } else if d < 40.0 { 1 } else if d < 80.0 { 2 } else { 3 }
    }

    /// Выдать игроку стартовый капитал (один раз при подключении).
    pub fn grant_start(&mut self, player: NetId) {
        self.credits.entry(player).or_insert(1000);
    }
}

/// Поза пешего персонажа (для репликации другим клиентам).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterPose {
    /// Позиция в серверных единицах (как и позиция корабля).
    pub pos: Vec3,
    /// Курс (радианы).
    pub yaw: f32,
    /// Идёт ли персонаж (для анимации у других).
    pub moving: bool,
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
            target_spawns: BTreeMap::new(),
            target_respawn_tick: BTreeMap::new(),
            characters: BTreeMap::new(),
            planet: ou_sim::home_planet(),
            economy: Economy::new(),
            market_dirty: true,
            pending_explosions: Vec::new(),
            explosion_seq: 0,
            next_entity_id: 1,
        }
    }

    /// Выкопать воронку в направлении `dir` (авторитетно).
    ///
    /// Сервер применяет воронку к своему рельефу планеты, **перепривязывает**
    /// рельеф всем сущностям-на-планете (чтобы физика видела новую форму и
    /// посадка/столкновения учитывали яму) и возвращает правку для рассылки.
    /// Слот выбирается по направлению: удары в одну точку углубляют ту же яму.
    pub fn dig(&mut self, slot: u32, dir: Vec3, cos_radius: f64, depth: f64) -> Dent {
        let d = ou_sim::normalize(dir);
        let dent = Dent { x: d.x, y: d.y, z: d.z, cos_r: cos_radius.clamp(-1.0, 1.0), depth };
        Dent::place(&mut self.planet.dents, &mut self.planet.dent_count, slot as usize, dent);
        // Перепривязываем обновлённый рельеф всем, кто на планете.
        for ent in self.world.entities.values_mut() {
            if ent.terrain.is_some() {
                ent.terrain = Some(self.planet);
            }
        }
        dent
    }

    /// Взрыв: крупная воронка в точке `center` (серверные единицы).
    ///
    /// В отличие от копания, взрыв бьёт **по точке** (а не по направлению):
    /// находим направление от центра планеты к этой точке, считаем угловой
    /// радиус большого кратера и вычитаем его из рельефа. Используется боевыми
    /// попаданиями/уничтожением у поверхности. Возвращает применённую воронку.
    pub fn explode(&mut self, slot: u32, center: Vec3, radius_units: f64, depth_units: f64) -> Dent {
        let c = self.planet.center;
        let rel = center.sub(c);
        let dir = ou_sim::normalize(rel);
        // Радиус планеты (в серверных единицах) — из параметров рельефа.
        let r = self.planet.params.radius;
        // Косинус углового радиуса кратера: чем больше радиус, тем меньше cos.
        let cos_r = (1.0 - (radius_units * radius_units) / (2.0 * r * r)).clamp(-1.0, 1.0);
        let dent = Dent { x: dir.x, y: dir.y, z: dir.z, cos_r, depth: depth_units };
        Dent::place(&mut self.planet.dents, &mut self.planet.dent_count, slot as usize, dent);
        for ent in self.world.entities.values_mut() {
            if ent.terrain.is_some() {
                ent.terrain = Some(self.planet);
            }
        }
        dent
    }
    /// Выкопать грунт игроком: рельеф + добыча руды.
    ///
    /// Возвращает (применённая воронка, что добыто): (id предмета, количество).
    /// Руда залегает **слоями по глубине** (см. `Economy::ore_at_depth`): у
    /// поверхности — руда, глубже — лёд, кристалл, металл.
    pub fn dig_as(&mut self, player: NetId, slot: u32, dir: Vec3, cos_radius: f64, depth: f64) -> (Dent, (u32, u64), f64) {
        let dent = self.dig(slot, dir, cos_radius, depth);
        let sys = self.economy.system;
        // Глубина ямы в ЭТОЙ точке — СУММАРНАЯ по слоту (1 ед = 1 м). Именно она
        // определяет слой, а не глубина одного удара.
        let hole_depth = self
            .planet
            .dents
            .get(slot as usize)
            .map(|d| d.depth)
            .unwrap_or(dent.depth);
        let (item, base_qty) = self.economy.ore_at_depth(hole_depth);
        // Бонус за глубину удара: глубже — больше за удар.
        let bonus = (depth / 10.0).clamp(0.0, 20.0) as u64;
        let qty = base_qty + bonus;
        self.economy.engine.inventory.add(player, sys, item, qty);
        self.market_dirty = true;
        (dent, (item, qty), hole_depth)
    }

    /// Разместить ордер на рынке от имени игрока.
    ///
    /// Авторитетно: продажа списывает предметы сразу (эскроу), покупка
    /// исполняется против стакана, при исполнении кредиты переходят продавцу,
    /// предметы — покупателю. Возвращает (исполнено, ошибка), ошибка — строка.
    pub fn place_order(
        &mut self,
        player: NetId,
        side: u8,
        item: u32,
        price: u64,
        qty: u64,
    ) -> Result<u64, String> {
        let sys = self.economy.system;
        let tick = self.world.tick;
        let side = if side == 0 { Side::Buy } else { Side::Sell };
        // Кредиты: у покупателя должно хватать на весь ордер (MVP-проверка).
        if side == Side::Buy {
            let have = *self.economy.credits.get(&player).unwrap_or(&0);
            let need = price.saturating_mul(qty);
            if have < need {
                return Err(format!("не хватает кредитов: есть {have}, нужно {need}"));
            }
        }
        let report = match side {
            Side::Buy => self
                .economy
                .engine
                .place_buy(sys, item, player, price, qty, tick),
            Side::Sell => self
                .economy
                .engine
                .place_sell(sys, item, player, price, qty, tick),
        };
        let report = match report {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };
        // Перевод кредитов по каждой сделке. Предметы движок рынка уже
        // перевёл сам (`settle`): покупателю прибавил, у продавца списал при
        // размещении ордера продажи.
        for t in &report.trades {
            let total = t.total();
            *self.economy.credits.entry(t.buyer).or_insert(0) =
                self.economy.credits.get(&t.buyer).copied().unwrap_or(0).saturating_sub(total);
            *self.economy.credits.entry(t.seller).or_insert(0) =
                self.economy.credits.get(&t.seller).copied().unwrap_or(0).saturating_add(total);
        }
        self.market_dirty = true;
        Ok(report.filled)
    }

    /// Состояние экономики игрока для UI: кредиты, инвентарь, верх стакана.
    pub fn market_state(&self, player: NetId) -> (u64, Vec<(u32, u64)>, Vec<(u32, u64, u64)>, Vec<(u32, u64, u64)>) {
        let sys = self.economy.system;
        let credits = *self.economy.credits.get(&player).unwrap_or(&0);
        let mut items = Vec::new();
        for i in 1..=self.economy.registry.len() as u32 {
            let a = self.economy.engine.inventory.amount(player, sys, i);
            if a > 0 {
                items.push((i, a));
            }
        }
        let mut bids = Vec::new();
        let mut asks = Vec::new();
        for i in 1..=self.economy.registry.len() as u32 {
            if let Some(l) = self.economy.engine.market.best_bid(sys, i) {
                bids.push((i, l.price, l.qty));
            }
            if let Some(l) = self.economy.engine.market.best_ask(sys, i) {
                asks.push((i, l.price, l.qty));
            }
        }
        (credits, items, bids, asks)
    }

    /// Список рецептов производства (id, имя, входы, выход, тики) для UI.
    pub fn recipe_list(&self) -> Vec<(u32, String, Vec<(u32, u64)>, (u32, u64), u64)> {
        let sys = self.economy.system;
        let _ = sys;
        self.economy
            .production
            .iter()
            .map(|r| (r.id, r.name.clone(), r.inputs.clone(), r.output, r.ticks))
            .collect()
    }

    /// Выполнить рецепт производства для игрока (авторитетно).
    ///
    /// Списывает входы из инвентаря и начисляет выход. Возвращает (id, кол-во)
    /// выходного предмета или строку-ошибку.
    pub fn craft_as(&mut self, player: NetId, recipe: u32) -> Result<(u32, u64), String> {
        let sys = self.economy.system;
        let inv = &mut self.economy.engine.inventory;
        match self.economy.production.craft(recipe, inv, player, sys) {
            Ok(out) => {
                self.market_dirty = true;
                Ok(out)
            }
            Err(e) => Err(e.to_string()),
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

        // Ближайшая цель в пределах дальности (кроме себя, уничтоженных и
        // неуязвимых после возрождения).
        let mut best: Option<(NetId, f64)> = None;
        for (id, ent) in self.world.entities.iter() {
            if *id == attacker {
                continue;
            }
            if self.hulls.get(id).map(|h| h.is_destroyed()).unwrap_or(true) {
                continue;
            }
            // Неуязвимый после возрождения игрок не получает урона.
            if self
                .connections
                .values()
                .any(|c| c.entity_id == *id && tick < c.invuln_until)
            {
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
        let target_pos = self.world.entities.get(&target)?.state.pos;

        let target_hull = *self.hulls.get(&target)?;
        let outcome = ou_combat::resolve_shot(&target_hull, &w, dist, seed);
        if let Some(h) = self.hulls.get_mut(&target) {
            ou_combat::apply_outcome(h, &outcome);
        }
        let destroyed = self.hulls.get(&target).map(|h| h.is_destroyed()).unwrap_or(false);
        let damage = outcome.shield_damage + outcome.armor_damage + outcome.core_damage;

        // Уничтоженный дрон-цель убираем из мира и планируем его возрождение
        // (учебный полигон — цели возвращаются). Игрока не удаляем: срок его
        // возрождения назначит `process_respawns`.
        if destroyed && !self.connections.values().any(|c| c.entity_id == target) {
            self.despawn(target);
            if self.target_spawns.contains_key(&target) {
                self.target_respawn_tick.insert(target, tick + RESPAWN_TICKS);
            }
        }

        // Взрыв у поверхности: уничтоженная цель оставляет крупную воронку в
        // рельефе (как `dig`, но по точке и крупнее). Только если цель низко над
        // землёй — в космосе рельефа нет.
        if destroyed {
            let dir = ou_sim::normalize(target_pos.sub(self.planet.center));
            let r = self.planet.params.radius;
            let surf = self.planet.height(dir);
            if target_pos.length() - (r + surf) < EXPLOSION_ALT_LIMIT {
                let slot = EXPLOSION_SLOT_BASE + (self.explosion_seq % EXPLOSION_SLOTS);
                self.explosion_seq += 1;
                let dent = self.explode(slot, target_pos, EXPLOSION_RADIUS, EXPLOSION_DEPTH);
                self.pending_explosions.push((slot, dent));
            }
        }

        Some(ShotEvent {
            attacker,
            target,
            hit: outcome.hit,
            damage,
            destroyed,
        })
    }

    /// Обработать выстрел дрона: цель — только игроки (сущности с соединением).
    ///
    /// Логика та же, что у [`resolve_fire`], но набор целей ограничен игроками,
    /// чтобы дроны не стреляли друг в друга.
    pub fn resolve_fire_at_players(&mut self, attacker: NetId, tick: u64) -> Option<ShotEvent> {
        let w = self.weapons.get(&attacker)?.clone();
        if let Some(last) = self.last_fire_tick.get(&attacker) {
            if tick.saturating_sub(*last) < w.cooldown {
                return None;
            }
        }
        let from = self.world.entities.get(&attacker)?.state.pos;

        let mut best: Option<(NetId, f64)> = None;
        for c in self.connections.values() {
            let id = c.entity_id;
            if id == attacker {
                continue;
            }
            if self.hulls.get(&id).map(|h| h.is_destroyed()).unwrap_or(true) {
                continue;
            }
            if tick < c.invuln_until {
                continue;
            }
            let Some(ent) = self.world.entities.get(&id) else { continue };
            let d = from.distance(ent.state.pos);
            if d <= w.range {
                match best {
                    Some((_, bd)) if bd <= d => {}
                    _ => best = Some((id, d)),
                }
            }
        }

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
        Some(ShotEvent { attacker, target, hit: outcome.hit, damage, destroyed })
    }

    /// Возрождение и восстановление щита. Возвращает (мёртвые, возрождённые).
    ///
    /// Уничтоженные игроки ждут `RESPAWN_TICKS`, затем «встают» с полным
    /// корпусом и краткой неуязвимостью. Щит живых медленно восстанавливается.
    fn process_respawns(&mut self, tick: u64) -> (Vec<NetId>, Vec<NetId>) {
        let mut died = Vec::new();
        let mut respawned = Vec::new();
        let ids: Vec<NetId> = self.connections.values().map(|c| c.entity_id).collect();
        for id in ids {
            let (respawn_tick, invuln_until) = match self.connections.values().find(|c| c.entity_id == id) {
                Some(c) => (c.respawn_tick, c.invuln_until),
                None => continue,
            };
            let dead = self.hulls.get(&id).map(|h| h.is_destroyed()).unwrap_or(false);
            if dead && respawn_tick == 0 {
                // Только что уничтожен — назначим срок и сообщим.
                let rt = tick + RESPAWN_TICKS;
                for c in self.connections.values_mut() {
                    if c.entity_id == id {
                        c.respawn_tick = rt;
                    }
                }
                died.push(id);
            } else if dead && tick >= respawn_tick {
                // Возрождение: полный корпус + неуязвимость.
                self.hulls.insert(id, ShipHull::new(100, 200));
                if let Some(spawn) = self.respawn_point(id) {
                    if let Some(ent) = self.world.entities.get_mut(&id) {
                        ent.state.pos = spawn;
                        ent.state.vel = Vec3::ZERO;
                    }
                    self.world.attach_terrain(id, ou_sim::home_planet());
                }
                let until = tick + RESPAWN_INVULN_TICKS;
                for c in self.connections.values_mut() {
                    if c.entity_id == id {
                        c.respawn_tick = 0;
                        c.invuln_until = until;
                    }
                }
                respawned.push(id);
            } else if !dead && invuln_until != 0 && tick >= invuln_until {
                for c in self.connections.values_mut() {
                    if c.entity_id == id {
                        c.invuln_until = 0;
                    }
                }
            }
            // Регенерация щита живых: медленно (1 единица за 30 тиков = 1 с),
            // иначе бой никогда не заканчивается победой.
            if !dead {
                if let Some(h) = self.hulls.get_mut(&id) {
                    h.recharge(if tick % 30 == 0 { 1 } else { 0 });
                }
            }
        }
        (died, respawned)
    }

    /// Точка возрождения игрока — домашняя планета (на суше).
    fn respawn_point(&self, _entity: NetId) -> Option<Vec3> {
        Some(ou_sim::home_spawn(2.0))
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
        // Стартовый капитал и право торговать.
        self.economy.grant_start(player_id);
        self.connections.insert(
            player_id,
            Connection {
                player_id,
                entity_id,
                last_input: InputFrame::neutral(0),
                acked_seq: 0,
                known: BTreeSet::new(),
                last_sent: BTreeMap::new(),
                respawn_tick: 0,
                invuln_until: 0,
            },
        );
        entity_id
    }

    /// Описание звёздной системы для отправки клиенту (для отрисовки).
    ///
    /// В текущей версии мира планета **одна**, и она же — домашняя: клиент
    /// строит её сам из общих констант (`HOME_PLANET_*`), поэтому дополнительных
    /// тел в системе нет. Сообщение остаётся ради имени/идентификатора системы
    /// и на будущее (когда появятся другие тела).
    pub fn system_description(&self) -> (u32, String, Vec<ou_protocol::SystemBodyDto>) {
        // Реальная домашняя система: звезда, луна, соседняя планета, газовый
        // гигант — на своих расстояниях (сотни тысяч км). Домашняя планета
        // (тело 90) идёт первой — на ней база и точка появления.
        let sys = ou_sim::home_system();
        let mut bodies = Vec::new();
        // Домашняя планета: в начале координат, радиус — как у рельефа.
        bodies.push(ou_protocol::SystemBodyDto {
            id: ou_sim::home_body_id(),
            kind: 1,          // планета
            surface: 0,       // землеподобная
            radius: ou_sim::HOME_PLANET_RADIUS,
            center: Vec3::ZERO,
            orbit: 0.0,
            phase: 0.0,
            omega: 0.0,
        });
        for b in &sys.bodies {
            let (kind, surface) = match b.kind {
                ou_sim::BodyKind::Star => (0u8, 0u8),
                ou_sim::BodyKind::Planet => (1, 0),
                ou_sim::BodyKind::Moon => (2, 5),
                ou_sim::BodyKind::GasGiant => (3, 4),
                ou_sim::BodyKind::AsteroidBelt => (4, 5),
                ou_sim::BodyKind::BlackHole => (5, 5),
            };
            bodies.push(ou_protocol::SystemBodyDto {
                id: b.id,
                kind,
                surface,
                radius: b.radius,
                center: b.pos,
                orbit: b.orbit as f32,
                phase: b.phase as f32,
                omega: 0.0,
            });
        }
        (sys.id, sys.name, bodies)
    }

    /// Подключение игрока **на домашней планете** (точка появления на суше).
    ///
    /// Корабль привязывается к рельефу: работают гравитация и посадка.
    /// Используется реальным сервером (`ou-netd`).
    pub fn connect_on_planet(&mut self, player_id: NetId) -> NetId {
        // Небольшой зазор над поверхностью: гравитация мягко опустит корабль.
        let entity_id = self.connect(player_id, ou_sim::home_spawn(2.0));
        // Привязываем АВТОРИТЕТНЫЙ рельеф мира (с уже выкопанными воронками),
        // а не чистую формулу: иначе игрок не видел бы чужие ямы в физике.
        self.world.attach_terrain(entity_id, self.planet);
        entity_id
    }

    /// Создать цель-дрон (мишень без игрока).
    ///
    /// У дрона есть корпус (без щита, средняя броня) и оружие: он служит целью
    /// для проверки боя и **отстреливается** по ближайшему игроку. Дрон висит
    /// в воздухе (гравитацию к нему не привязываем).
    pub fn spawn_target(&mut self, pos: Vec3) -> NetId {
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;
        self.world.spawn(entity_id, pos);
        self.hulls.insert(entity_id, ShipHull::new(0, 40));
        if let Some(w) = weapon("лазер") {
            self.weapons.insert(entity_id, w);
        }
        // Учебная цель: помним точку спавна — после уничтожения возродится.
        self.target_spawns.insert(entity_id, pos);
        entity_id
    }

    /// Респавн уничтоженных учебных целей на их точках появления.
    fn process_target_respawns(&mut self, tick: u64) {
        let due: Vec<NetId> = self
            .target_respawn_tick
            .iter()
            .filter(|(_, t)| tick >= **t)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            if let Some(spawn) = self.target_spawns.get(&id).copied() {
                if let Some(ent) = self.world.entities.get_mut(&id) {
                    ent.state.pos = spawn;
                    ent.state.vel = Vec3::ZERO;
                    self.hulls.insert(id, ShipHull::new(0, 40));
                } else {
                    // Был удалён — создаём заново под тем же id.
                    self.world.spawn(id, spawn);
                    self.hulls.insert(id, ShipHull::new(0, 40));
                    if let Some(w) = weapon("лазер") {
                        self.weapons.insert(id, w);
                    }
                }
            }
            self.target_respawn_tick.remove(&id);
        }
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

    /// Приём позы пешего персонажа от клиента.
    ///
    /// `active = false` — игрок снова в корабле, позу снимаем.
    pub fn receive_character(
        &mut self,
        entity: NetId,
        pos: Vec3,
        yaw: f32,
        moving: bool,
        active: bool,
    ) {
        if active {
            self.characters.insert(entity, CharacterPose { pos, yaw, moving });
        } else {
            self.characters.remove(&entity);
        }
    }

    /// Один авторитетный тик: шаг мира + формирование рассылки.
    pub fn tick(&mut self) -> SendPlan {
        // 0. Возрождение и восстановление: мёртвые ждут срок, живые регенят щит.
        let tick_now = self.world.tick;
        let (died, respawned) = self.process_respawns(tick_now);
        self.process_target_respawns(tick_now);

        // Состояние экономики по игрокам (только если она менялась): считаем
        // заранее, чтобы не конфликтовать с мутабельным проходом по соединениям.
        let market_states: BTreeMap<NetId, Message> = if self.market_dirty {
            self.connections
                .keys()
                .map(|pid| {
                    let (credits, items, bids, asks) = self.market_state(*pid);
                    (*pid, Message::ServerMarket { credits, items, bids, asks })
                })
                .collect()
        } else {
            BTreeMap::new()
        };

        // 1. Собрать вводы по сущностям.
        let mut inputs: BTreeMap<NetId, InputFrame> = BTreeMap::new();
        for conn in self.connections.values() {
            // Мёртвый игрок ввод не применяет (корабль уничтожен).
            if conn.respawn_tick != 0 {
                continue;
            }
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

        // 3b. Дроны (сущности без соединения) отстреливаются по ближайшему
        //     игроку — так бой двусторонний и видно смерть/возрождение.
        let drone_ids: Vec<NetId> = self
            .world
            .entities
            .keys()
            .copied()
            .filter(|id| !self.connections.values().any(|c| c.entity_id == *id))
            .filter(|id| self.weapons.contains_key(id))
            .collect();
        for id in drone_ids {
            if let Some(ev) = self.resolve_fire_at_players(id, tick) {
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

        // Снимки сроков для сообщений о смерти/возрождении (до заимствования
        // соединений на изменение).
        let died_respawn: BTreeMap<NetId, u64> = self
            .connections
            .values()
            .map(|c| (c.entity_id, c.respawn_tick))
            .collect();
        let respawn_until: BTreeMap<NetId, u64> = self
            .connections
            .values()
            .map(|c| (c.entity_id, c.invuln_until))
            .collect();

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

        // Взрывы этого тика — снимок для рассылки (все, кто видит место).
        let explosions: Vec<(u32, Dent)> = std::mem::take(&mut self.pending_explosions);

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
            // Взрывы: воронки от боя рассылаем тем же сообщением, что и копание.
            for (slot, dent) in &explosions {
                msgs.push(Message::ServerTerrainDig {
                    slot: *slot,
                    dir: Vec3::new(dent.x, dent.y, dent.z),
                    cos_radius: dent.cos_r,
                    depth: dent.depth,
                });
            }
            // Смерть и возрождение: шлём владельцу сущности (и тем, кто видит).
            for e in &died {
                if conn.entity_id == *e || visible.contains(e) {
                    msgs.push(Message::ServerDeath {
                        entity: *e,
                        respawn_tick: died_respawn.get(e).copied().unwrap_or(0),
                    });
                }
            }
            for e in &respawned {
                if conn.entity_id == *e || visible.contains(e) {
                    msgs.push(Message::ServerRespawn {
                        entity: *e,
                        invuln_until: respawn_until.get(e).copied().unwrap_or(0),
                    });
                }
            }
            // Позы пеших персонажей других игроков (видимых рядом).
            for (id, pose) in &self.characters {
                if conn.entity_id == *id {
                    continue; // своя поза не нужна
                }
                // Показываем, если сущность видна по позиции персонажа.
                let near = pose.pos.distance(
                    all_states.get(&conn.entity_id).map(|s| s.pos).unwrap_or(Vec3::ZERO),
                ) <= INTEREST_RADIUS;
                if visible.contains(id) || near {
                    msgs.push(Message::ServerCharacter {
                        entity: *id,
                        pos: pose.pos,
                        yaw: pose.yaw,
                        moving: pose.moving,
                    });
                }
            }

            // Обновляем «что клиент знает и в каком виде».
            conn.last_sent = visible.iter().map(|id| (*id, all_states[id])).collect();
            conn.known = visible;

            // Рынок: состояние экономики игрока шлём, когда оно менялось.
            if let Some(m) = market_states.get(player_id) {
                msgs.push(m.clone());
            }

            plan.messages.insert(*player_id, msgs);
        }
        if self.market_dirty {
            self.market_dirty = false;
        }

        plan
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

/// Вид поверхности тела (детерминированно, из id и типа) — задаёт палитру.
///
/// 0 землеподобная, 1 пустынная, 2 ледяная, 3 вулканическая, 4 газовый гигант,
/// 5 безвоздушная. Код совпадает с клиентским `EOUPlanetType`.
///
/// Сейчас в мире одна планета (её вид задан константами клиента), поэтому
/// функция не вызывается — сохранена на случай возврата тел системы.
#[allow(dead_code)]
fn surface_type_for(b: &ou_sim::Body) -> u8 {
    use ou_sim::BodyKind;
    match b.kind {
        BodyKind::GasGiant => 4,
        BodyKind::Moon => 5,          // спутники — безвоздушные
        BodyKind::AsteroidBelt => 5,
        BodyKind::Star | BodyKind::BlackHole => 0,
        BodyKind::Planet => {
            // Планеты различаем детерминированно по id: землеподобная/
            // пустынная/ледяная/вулканическая. Вращать по кругу безопасно и
            // воспроизводимо.
            match b.id % 4 {
                0 => 0,   // Terran
                1 => 1,   // Desert
                2 => 2,   // Ice
                _ => 3,   // Volcanic
            }
        }
    }
}

/// Сравнение состояний на уровне точности провода.
fn states_equal(a: &EntityState, b: &EntityState) -> bool {    a.pos.to_wire() == b.pos.to_wire()
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
            s.receive_input(2, &[InputFrame { seq: tick, throttle: 1.0, yaw: -1.5707963, pitch: 0.0, fire: false, ..Default::default() }]);
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
    fn dig_lowers_terrain_physics_and_is_idempotent_per_slot() {
        let mut s = Server::new();
        let e = s.connect_on_planet(1);
        let dir = ou_sim::home_spawn_direction();
        let before = s
            .world
            .entities
            .get(&e)
            .unwrap()
            .terrain
            .unwrap()
            .height(dir);
        // Копаем в направлении появления.
        let dent = s.dig(0, dir, 0.9995, 30.0);
        assert!(dent.depth > 0.0, "воронка должна иметь глубину");
        let after = s
            .world
            .entities
            .get(&e)
            .unwrap()
            .terrain
            .unwrap()
            .height(dir);
        assert!(after < before, "копание должно понизить поверхность: {before} -> {after}");
        // Удар в тот же слот углубляет ту же яму, а не плодит новую.
        let count_before = s.planet.dent_count;
        s.dig(0, dir, 0.9995, 30.0);
        assert_eq!(s.planet.dent_count, count_before, "повторный удар — тот же слот");
        let deeper = s
            .world
            .entities
            .get(&e)
            .unwrap()
            .terrain
            .unwrap()
            .height(dir);
        assert!(deeper < after, "повторный удар должен углубить яму: {after} -> {deeper}");
    }

    #[test]
    fn digging_yields_ore_and_market_trades_for_credits() {
        let mut s = Server::new();
        let a = s.connect_on_planet(1);   // продавец
        let b = s.connect_on_planet(2);   // покупатель
        let _ = a;
        let dir = ou_sim::home_spawn_direction();
        let ore = s.economy.ore_item;
        // Игрок 1 копает у поверхности (depth < 15 м) — получает РУДУ.
        for i in 0..5 {
            let (dent, (item, _), _) = s.dig_as(1, i, dir, 0.9995, 10.0);
            assert_eq!(item, ore, "у поверхности должна добываться руда");
            assert!(dent.depth <= 15.0);
        }
        let mined = s.economy.engine.inventory.amount(1, 1, ore);
        assert!(mined > 0, "копание должно давать руду, получено {mined}");

        let c1_start = *s.economy.credits.get(&1).unwrap();
        let c2_start = *s.economy.credits.get(&2).unwrap();
        assert!(c1_start > 0 && c2_start > 0, "стартовый капитал должен быть");

        // Игрок 1 продаёт руду по 10, игрок 2 покупает — сделка исполняется.
        let sold = s.place_order(1, /*side=Sell*/1, ore, 10, mined).unwrap();
        assert!(sold == 0, "ордер продажи просто встаёт в стакан");
        let bought = s.place_order(2, /*side=Buy*/0, ore, 10, mined).unwrap();
        assert_eq!(bought, mined, "покупка должна исполниться полностью");

        // Кредиты перешли продавцу, а руда — покупателю.
        assert_eq!(*s.economy.credits.get(&1).unwrap(), c1_start + mined * 10);
        assert_eq!(*s.economy.credits.get(&2).unwrap(), c2_start - mined * 10);
        assert_eq!(s.economy.engine.inventory.amount(2, 1, ore), mined);
        assert_eq!(s.economy.engine.inventory.amount(1, 1, ore), 0);
    }

    #[test]
    fn craft_uses_production_chain_in_game() {
        // Копаем руду, затем плавим металл в окне производства — вживую.
        let mut s = Server::new();
        s.connect_on_planet(1);
        let dir = ou_sim::home_spawn_direction();
        let ore = s.economy.ore_item;
        // Накопить руды (у поверхности — руда).
        for i in 0..6 { s.dig_as(1, i, dir, 0.9995, 10.0); }
        assert!(s.economy.engine.inventory.amount(1, 1, ore) >= 2, "нужно ≥2 руды");
        // Первый рецепт — «выплавка металла» (2 руды → 1 металл).
        let recipes = s.recipe_list();
        assert!(!recipes.is_empty(), "каталог рецептов не пуст");
        // Первый рецепт цепочки — «выплавка металла» (2 руды → 1 металл).
        let (recipe_id, ref _name, ref _inputs, (out_item, _), _) = recipes[0];
        let before = s.economy.engine.inventory.amount(1, 1, out_item);
        let res = s.craft_as(1, recipe_id);
        assert!(res.is_ok(), "крафт должен пройти: {:?}", res);
        assert_eq!(s.economy.engine.inventory.amount(1, 1, out_item), before + 1);

        // Крафт без входов — отказ (без падения).
        let bad = s.craft_as(1, 9999);
        assert!(bad.is_err(), "несуществующий рецепт должен быть отклонён");
    }

    #[test]
    fn explosion_carves_crater_in_terrain() {
        // Взрыв оставляет крупную воронку в рельефе (та же механика, что у боя).
        let mut s = Server::new();
        s.connect_on_planet(1);
        let dir = ou_sim::home_spawn_direction();
        let r = s.planet.params.radius;
        let ground = r + s.planet.height(dir);
        // Точка на земле (в серверных единицах).
        let center = dir.scale(ground);
        let before = s.planet.height(dir);
        let dent = s.explode(40, center, EXPLOSION_RADIUS, EXPLOSION_DEPTH);
        assert!(dent.depth > 0.0);
        let after = s.planet.height(dir);
        assert!(after < before, "взрыв должен углубить рельеф: {before} -> {after}");
        assert!(before - after > 1.0, "воронка заметная: {before} -> {after}");
    }

    #[test]
    fn market_state_is_pushed_after_dig() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.tick();   // первый тик: состояние рынка уходит
        let dir = ou_sim::home_spawn_direction();
        s.dig_as(1, 0, dir, 0.9995, 30.0);
        let plan = s.tick();
        let msgs = &plan.messages[&1];
        assert!(
            msgs.iter().any(|m| matches!(m, Message::ServerMarket { .. })),
            "после копания сервер должен прислать состояние экономики"
        );
    }

    #[test]
    fn dig_yield_depends_on_depth_layer() {
        // Полосы: 0..15 руда, 15..40 лёд, 40..80 кристалл, >80 металл.
        let mut s = Server::new();
        s.connect_on_planet(1);
        let dir = ou_sim::home_spawn_direction();
        let ore = s.economy.ore_item;
        let ice = s.economy.ice_item;
        let crystal = s.economy.crystal_item;
        let metal = s.economy.metal_item;

        // Один слот, удары всё глубже — слой добычи меняется вместе с глубиной.
        let (_, (i0, _), _) = s.dig_as(1, 0, dir, 0.9995, 10.0);   // ~10 м → руда
        assert_eq!(i0, ore);
        for _ in 0..6 { s.dig_as(1, 0, dir, 0.9995, 15.0); }    // яма глубже 80 м
        let deep = s.planet.dents[0].depth;
        assert!(deep > 80.0, "яма должна стать глубокой: {deep}");
        let (_, (i1, _), hole) = s.dig_as(1, 0, dir, 0.9995, 15.0);
        assert_eq!(i1, metal, "на большой глубине должна быть металлическая руда");
        assert!(hole > 80.0, "глубина ямы в ответе должна быть суммарной: {hole}");

        // Прямая проверка полос.
        assert_eq!(crate::Economy::ore_layer(5.0), 0);
        assert_eq!(crate::Economy::ore_layer(20.0), 1);
        assert_eq!(crate::Economy::ore_layer(60.0), 2);
        assert_eq!(crate::Economy::ore_layer(120.0), 3);
        // И что слои действительно дают разные предметы.
        let (a, _) = s.economy.ore_at_depth(5.0);
        let (b, _) = s.economy.ore_at_depth(20.0);
        let (c, _) = s.economy.ore_at_depth(60.0);
        let (d, _) = s.economy.ore_at_depth(120.0);
        assert_eq!(a, ore);
        assert_eq!(b, ice);
        assert_eq!(c, crystal);
        assert_eq!(d, metal);
    }

    #[test]
    fn fire_broadcasts_shot_to_nearby_player() {
        let mut s = Server::new();
        s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(100.0, 0.0, 0.0));
        s.tick(); // базовый снимок
        // Игрок 1 стреляет: ввод с fire.
        s.receive_input(1, &[InputFrame { seq: 1, throttle: 0.0, yaw: 0.0, pitch: 0.0, fire: true, ..Default::default() }]);
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

    #[test]
    fn player_respawns_after_destroyed() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        let b = s.connect(2, Vec3::new(60.0, 0.0, 0.0));
        // Взаимный огонь, пока кто-то не погибнет. Бьём игрока 2 напрямую:
        // сильно ослабим его корпус, чтобы он уничтожился быстрее.
        s.hulls.insert(b, ShipHull::new(0, 5));
        let mut tick = 0u64;
        let mut died = false;
        while tick < 400 {
            tick += 5;
            if let Some(ev) = s.resolve_fire(a, tick) {
                if ev.target == b && ev.destroyed {
                    died = true;
                    break;
                }
            }
        }
        assert!(died, "игрок 2 должен быть уничтожен");
        // Прогоняем тики: игрок должен возродиться с полным корпусом.
        let mut respawned = false;
        for _ in 0..(RESPAWN_TICKS + 10) {
            s.tick();
        }
        // Корпус восстановлен и не уничтожен.
        let h = s.hulls[&b];
        assert!(!h.is_destroyed(), "после возрождения корпус целый");
        assert!(h.armor > 0, "броня восстановлена");
        // Игрок всё ещё в мире (его не удаляют, в отличие от дронов).
        assert!(s.world.entities.contains_key(&b), "игрок остаётся в мире");
    }

    #[test]
    fn invulnerable_player_takes_no_damage() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        let b = s.connect(2, Vec3::new(60.0, 0.0, 0.0));
        // Помечаем игрока 2 неуязвимым (как сразу после возрождения).
        for c in s.connections.values_mut() {
            if c.entity_id == b {
                c.invuln_until = 10_000;
            }
        }
        let hull_before = s.hulls[&b];
        // Игрок 1 стреляет — но не должен найти неуязвимую цель.
        let ev = s.resolve_fire(a, 1).expect("выстрел в пустоту — событие");
        assert_eq!(ev.target, 0, "неуязвимая цель не выбирается");
        assert_eq!(s.hulls[&b], hull_before, "урон не прошёл");
    }

    #[test]
    fn drone_fires_back_at_player() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        let drone = s.spawn_target(Vec3::new(50.0, 0.0, 0.0));
        let hull_before = s.hulls[&a];
        // Дрон стреляет по игроку (цели — только игроки).
        let ev = s.resolve_fire_at_players(drone, 1).expect("дрон должен стрелять");
        assert_eq!(ev.target, a, "цель дрона — ближайший игрок");
        let hull_after = s.hulls[&a];
        assert!(
            hull_after.shield < hull_before.shield || hull_after.armor < hull_before.armor,
            "дрон должен нанести урон игроку"
        );
    }

    #[test]
    fn character_pose_is_replicated_to_other_players() {        let mut s = Server::new();
        let a = s.connect(1, Vec3::ZERO);
        s.connect(2, Vec3::new(20.0, 0.0, 0.0));
        s.tick(); // базовый снимок
        // Игрок 1 шлёт позу пешего персонажа рядом с игроком 2.
        s.receive_character(a, Vec3::new(0.0, 0.0, 0.0), 0.5, true, true);
        let plan = s.tick();
        let msgs = &plan.messages[&2];
        let seen = msgs.iter().any(|m| matches!(
            m,
            Message::ServerCharacter { entity, .. } if *entity == a
        ));
        assert!(seen, "игрок 2 должен увидеть персонажа игрока 1");
        // Снятие позы (игрок вернулся в корабль) — больше не рассылается.
        s.receive_character(a, Vec3::ZERO, 0.0, false, false);
        let plan2 = s.tick();
        let still = plan2.messages[&2].iter().any(|m| matches!(
            m,
            Message::ServerCharacter { entity, .. } if *entity == a
        ));
        assert!(!still, "после возврата в корабль поза не рассылается");
    }

    #[test]
    fn system_description_has_home_and_neighbours() {
        let s = Server::new();
        let (id, name, bodies) = s.system_description();
        assert_eq!(id, 0);
        assert!(!name.is_empty(), "у системы есть имя");
        // Первое тело — домашняя планета (в начале координат).
        assert_eq!(bodies[0].id, ou_sim::home_body_id());
        assert!(bodies[0].center.length() < 1.0, "домашняя планета в центре");
        // Есть соседние тела (луна/планета/звезда) — система не из одной планеты.
        assert!(bodies.len() >= 3, "в системе есть соседние тела: {}", bodies.len());
        // Есть и луна (тип 2), и звезда (тип 0) для ориентира.
        assert!(bodies.iter().any(|b| b.kind == 2), "есть луна");
        assert!(bodies.iter().any(|b| b.kind == 0), "есть звезда");
    }
}
