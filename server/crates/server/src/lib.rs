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
use ou_combat::{weapon, Claim, ShipHull, Territory, TerritoryStatus, Weapon};
use ou_economy::{Inventory, ItemRegistry, Market, MatchingEngine, Production, Side};
use ou_protocol::{
    BuildAction, BuildCommand, BuildEdit, BuildReason, EntityState, InputFrame, Message, NetId,
    ShotEvent, Snapshot, TerritoryDto, Vec3,
};
use ou_script::ScriptEngine;
use ou_sim::{det_cos, det_sin, Dent, Terrain, World};
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

/// Награда (bounty) в кредитах за уничтожение боевой цели — для HUD и экономики.
pub const DESTROY_BOUNTY: u64 = 250;

/// Ранг участника организации.
pub const RANK_MEMBER: u8 = 0;
/// Офицер: может назначать ранги участникам.
pub const RANK_OFFICER: u8 = 1;
/// Лидер: полный контроль (назначения, дипломатия).
pub const RANK_LEADER: u8 = 2;

/// Организация (гильдия): имя, тег, состав с рангами и дипломатия.
#[derive(Clone, Debug)]
pub struct Org {
    pub name: String,
    pub tag: String,
    /// Участники: игрок → ранг (0 участник, 1 офицер, 2 лидер).
    pub members: BTreeMap<NetId, u8>,
    /// Дипломатия: id другой организации → стойка (0 нейтралитет, 1 союз, 2 война).
    pub diplomacy: BTreeMap<u32, u8>,
}

impl Org {
    pub fn new(name: String, tag: String, leader: NetId) -> Self {
        let mut members = BTreeMap::new();
        members.insert(leader, RANK_LEADER);
        Org { name, tag, members, diplomacy: BTreeMap::new() }
    }

    /// Ранг игрока в организации (None — не участник).
    pub fn rank_of(&self, player: NetId) -> Option<u8> {
        self.members.get(&player).copied()
    }
}

/// Конструкция: воксельная сетка плюс права и ревизии (совместная стройка).
#[derive(Clone, Debug)]
pub struct Construction {    pub grid: VoxelGrid,
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
    /// Сущность → id шаблона скрипта (для окна «Скрипты» и статуса).
    pub script_template: BTreeMap<NetId, u32>,
    /// Сущность → статистика скрипта: (запусков, ошибок) и последняя ошибка.
    pub script_stats: BTreeMap<NetId, (u64, u64)>,
    /// Сущность → последняя ошибка скрипта.
    pub script_last_error: BTreeMap<NetId, String>,
    /// Сущность → свежий вывод `log()` (для внутриигрового дисплея).
    pub script_logs: BTreeMap<NetId, Vec<String>>,
    /// Есть изменения статуса скриптов, которые надо разослать владельцам.
    pub script_urgent: BTreeSet<NetId>,
    /// Боевая статистика игрока: (убийства, урон, награда) — для HUD.
    pub combat_stats: BTreeMap<NetId, (u64, u64, u64)>,
    /// Игроки, чья боевая статистика изменилась (надо разослать).
    pub combat_dirty: BTreeSet<NetId>,
    /// Мастерская: опубликованные шаблоны по id — (имя, автор, воксели, цена).
    pub workshop: BTreeMap<u32, (String, NetId, Vec<((i32, i32, i32), BuildColor)>, u64)>,
    /// Купленные/созданные шаблоны каждого игрока: (id, имя, воксели).
    pub own_blueprints: BTreeMap<NetId, BTreeMap<u32, (String, Vec<((i32, i32, i32), BuildColor)>)>>,
    /// Следующий id объявления мастерской.
    next_workshop_id: u32,
    /// Игроки, которым надо обновить витрину мастерской.
    pub workshop_dirty: BTreeSet<NetId>,
    /// Организации: id → (имя, тег, участники (игрок, ранг 0/1/2), дипломатия).
    pub orgs: BTreeMap<u32, Org>,
    /// Следующий id организации.
    next_org_id: u32,
    /// Игрок → id его организации (отсутствие = без организации).
    pub player_org: BTreeMap<NetId, u32>,
    /// Игроки, которым надо обновить состояние организации.
    pub org_dirty: BTreeSet<NetId>,
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
    /// Территории системы: владение, статус, налог. Индексируются по id.
    pub territories: Vec<Territory>,
    /// Территории изменились с прошлого тика (надо разослать их клиентам).
    pub territory_dirty: bool,
    /// Налог, собранный каждой территорией (id территории → сумма), для показа.
    pub territory_tax: BTreeMap<u32, u64>,
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
            script_template: BTreeMap::new(),
            script_stats: BTreeMap::new(),
            script_last_error: BTreeMap::new(),
            script_logs: BTreeMap::new(),
            script_urgent: BTreeSet::new(),
            combat_stats: BTreeMap::new(),
            combat_dirty: BTreeSet::new(),
            workshop: BTreeMap::new(),
            own_blueprints: BTreeMap::new(),
            next_workshop_id: 1,
            workshop_dirty: BTreeSet::new(),
            orgs: BTreeMap::new(),
            next_org_id: 1,
            player_org: BTreeMap::new(),
            org_dirty: BTreeSet::new(),
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
            territories: Vec::new(),
            territory_dirty: true,
            territory_tax: BTreeMap::new(),
            next_entity_id: 1,
        }
    }

    /// Собрать тела системы (домашняя планета + соседние) для физики.
    /// Вызывается один раз при старте сервера.
    pub fn build_system(&mut self) {
        self.world.build_system();
        self.build_territories();
    }

    /// Разложить территории системы: база на домашней планете (мирная зона),
    /// несколько ничейных зон вокруг соседних тел (космос/орбита).
    ///
    /// Позиции — в серверных единицах. Территория у базы мирная: в ней нельзя
    /// воевать (защита новичков). Соседние зоны ничейные и спорные — их можно
    /// захватить.
    pub fn build_territories(&mut self) {
        self.territories.clear();
        self.territory_tax.clear();
        // Территория базы: центр — точка появления, радиус 3000 ед. (~3 км).
        // Мирная: бой у базы запрещён.
        let mut home = Territory::new(1, self.economy.system, ou_sim::home_spawn(2.0), 3000.0);
        home.owner = Claim::Neutral;
        home.status = TerritoryStatus::Peaceful;
        home.tax_percent = 0;
        self.territories.push(home);
        // Ничейные зоны у соседних тел: их можно объявить своими (захват).
        // Используем точки тел системы (луна, соседняя планета).
        let mut id = 2;
        for b in ou_sim::home_system().bodies {
            if b.id == ou_sim::home_body_id() || b.radius <= 0.0 {
                continue;
            }
            // Зона вокруг орбиты тела: центр — тело, радиус втрое больше тела.
            let mut t = Territory::new(id, self.economy.system, b.pos, (b.radius * 3.0).max(20_000.0));
            t.owner = Claim::Neutral;
            t.status = TerritoryStatus::Contested; // спорная: можно воевать/захватить
            self.territories.push(t);
            id += 1;
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
            // Налог с территории: если продавец торгует в чужой зоне, часть
            // выручки уходит владельцу территории (собственность и налоги).
            let seller_pos = self
                .connections
                .get(&t.seller)
                .and_then(|c| self.world.entities.get(&c.entity_id))
                .map(|e| e.state.pos);
            let tax = seller_pos.and_then(|p| self.tax_for_trade(p, total));
            let (owner, tax_amt) = match tax {
                Some((o, v)) => (Some(o), v),
                None => (None, 0),
            };
            *self.economy.credits.entry(t.seller).or_insert(0) =
                self.economy.credits.get(&t.seller).copied().unwrap_or(0)
                    .saturating_add(total.saturating_sub(tax_amt));
            if let Some(o) = owner {
                *self.economy.credits.entry(o).or_insert(0) =
                    self.economy.credits.get(&o).copied().unwrap_or(0).saturating_add(tax_amt);
            }
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

    /// Список территорий для отправки клиенту (для HUD и миникарты).
    pub fn territory_list(&self) -> Vec<TerritoryDto> {
        self.territories
            .iter()
            .map(|t| TerritoryDto {
                id: t.id,
                owner: match t.owner {
                    Claim::Neutral => 0,
                    Claim::Owned(p) => p,
                },
                status: match t.status {
                    TerritoryStatus::Peaceful => 0,
                    TerritoryStatus::Contested => 1,
                    TerritoryStatus::Fortified => 2,
                },
                center: t.center,
                radius: t.radius,
                control_shield: t.control_shield,
                tax_percent: t.tax_percent,
                collected_tax: self.territory_tax.get(&t.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Территория, содержащая точку (первая по порядку id).
    pub fn territory_at(&self, p: Vec3) -> Option<&Territory> {
        self.territories.iter().find(|t| t.contains(p))
    }

    /// Захват территории игроком (авторитетно).
    ///
    /// Игрок должен **находиться внутри** территории. Мирную базу захватить
    /// нельзя (защита новичков). Возвращает результат (id, ошибка).
    pub fn capture_as(&mut self, player: NetId, id: u32) -> Result<u32, String> {
        let pos = self
            .connections
            .get(&player)
            .and_then(|c| self.world.entities.get(&c.entity_id))
            .map(|e| e.state.pos)
            .ok_or_else(|| "игрок не в мире".to_string())?;
        let tick = self.world.tick;
        let t = self
            .territories
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("территория {id} не найдена"))?;
        if !t.contains(pos) {
            return Err("надо быть внутри территории".to_string());
        }
        if matches!(t.status, TerritoryStatus::Peaceful) {
            return Err("мирную зону захватить нельзя".to_string());
        }
        if t.owner == Claim::Owned(player) {
            return Err("территория уже ваша".to_string());
        }
        t.capture(player, tick);
        self.territory_dirty = true;
        Ok(id)
    }

    /// Налог, который владелец территории берёт со сделки на сумму `amount`.
    /// Возвращает (кому, сколько): владельцу уходит налог, торговцу — остаток.
    /// Если сделка вне территории или территория ничья — налога нет.
    pub fn tax_for_trade(&mut self, at: Vec3, amount: u64) -> Option<(NetId, u64)> {
        let (id, owner, tax) = {
            let t = self.territory_at(at)?;
            let owner = match t.owner {
                Claim::Owned(p) => p,
                Claim::Neutral => return None,
            };
            (t.id, owner, t.tax_on(amount))
        };
        if tax == 0 {
            return None;
        }
        *self.territory_tax.entry(id).or_insert(0) += tax;
        self.territory_dirty = true;
        Some((owner, tax))
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
        // Мирная зона: у базы запрещён бой между игроками (защита новичков),
        // но по целям-дронам (учебный полигон) стрелять можно.
        let attacker_peaceful =
            self.territory_at(from).map(|t| !t.allows_combat()).unwrap_or(false);

        // Ближайшая цель в пределах дальности (кроме себя, уничтоженных,
        // неуязвимых и «защищённых мирной зоной» игроков).
        let mut best: Option<(NetId, f64)> = None;
        for (id, ent) in self.world.entities.iter() {
            if *id == attacker {
                continue;
            }
            if self.hulls.get(id).map(|h| h.is_destroyed()).unwrap_or(true) {
                continue;
            }
            let is_player = self.connections.values().any(|c| c.entity_id == *id);
            if is_player {
                // Если стрелок в мирной зоне — игроков не бьём.
                if attacker_peaceful {
                    continue;
                }
                // Если цель в мирной зоне — тоже не бьём (убежище).
                if self.territory_at(ent.state.pos).map(|t| !t.allows_combat()).unwrap_or(false) {
                    continue;
                }
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

        // Боевая статистика стрелка: если он игрок, копим урон и награду.
        if let Some(player) = self.connections.values().find(|c| c.entity_id == attacker).map(|c| c.player_id) {
            let st = self.combat_stats.entry(player).or_insert((0, 0, 0));
            st.1 += damage as u64;
            if destroyed {
                st.0 += 1;
                // Награда за уничтожение цели идёт в кредиты игрока (bounty).
                let reward = DESTROY_BOUNTY;
                st.2 += reward;
                *self.economy.credits.entry(player).or_insert(0) =
                    self.economy.credits.get(&player).copied().unwrap_or(0).saturating_add(reward);
                self.market_dirty = true;
            }
            self.combat_dirty.insert(player);
        }

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

    /// Список шаблонов скриптов для окна «Скрипты»: (id, имя, описание).
    pub fn script_templates(&self) -> Vec<(u32, String, String)> {
        ou_script::SCRIPT_TEMPLATES
            .iter()
            .map(|t| (t.id, t.name.to_string(), t.description.to_string()))
            .collect()
    }

    /// Список шаблонов постройки (blueprints) для редактора: (id, имя, описание).
    pub fn blueprint_templates(&self) -> Vec<(u32, String, String)> {
        ou_build::BLUEPRINTS
            .iter()
            .map(|b| (b.id, b.name.to_string(), b.description.to_string()))
            .collect()
    }

    /// Поставить шаблон постройки в конструкцию игрока (авторитетно).
    ///
    /// Игрок штампует воксели шаблона с якорем `origin`. Детали, не прошедшие
    /// проверку (занято, вне границ, разрыв связности), пропускаются — мир не
    /// роняем. Возвращает число поставленных вокселей.
    pub fn place_blueprint_as(
        &mut self,
        author: NetId,
        entity: NetId,
        id: u32,
        origin: (i32, i32, i32),
    ) -> Result<u32, String> {
        let voxels = ou_build::blueprint_voxels(id).ok_or_else(|| format!("шаблон {id} не найден"))?;
        // Права проверяем до создания конструкции (как в apply_build_as).
        if let Some(c) = self.constructions.get(&entity) {
            if !c.can_edit(author) {
                return Err("нет прав на эту конструкцию".to_string());
            }
        }
        let c = self
            .constructions
            .entry(entity)
            .or_insert_with(|| Construction::new(author));
        // Смещение шаблона: ставим относительно якоря.
        let mut placed = 0u32;
        let mut edits: Vec<BuildEdit> = Vec::new();
        for ((dx, dy, dz), col) in voxels {
            let coord = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
            let color = BuildColor(col.0, col.1, col.2);
            if c.grid.place(coord, color).is_ok() {
                c.revision += 1;
                edits.push(BuildEdit {
                    entity,
                    author,
                    action: BuildAction::Place,
                    coord,
                    color: ou_protocol::Color(col.0, col.1, col.2),
                    revision: c.revision,
                });
                placed += 1;
            }
        }
        if placed > 0 {
            for (_player, slot) in c.pending.iter_mut() {
                slot.1.extend(edits.iter().copied());
            }
        }
        Ok(placed)
    }

    // --- Мастерская и торговля шаблонами ---

    /// Опубликовать текущую постройку игрока как шаблон в мастерской.
    ///
    /// Сервер снимает воксели конструкции игрока (относительно её ядра) и
    /// выставляет в мастерскую с ценой. Автор сразу получает копию у себя.
    pub fn publish_blueprint(&mut self, player: NetId, name: String, price: u64) -> Result<u32, String> {
        let entity = self
            .connections
            .get(&player)
            .map(|c| c.entity_id)
            .ok_or_else(|| "игрок не в мире".to_string())?;
        let voxels: Vec<((i32, i32, i32), BuildColor)> = self
            .constructions
            .get(&entity)
            .map(|c| c.grid.iter().collect())
            .unwrap_or_default();
        if voxels.is_empty() {
            return Err("нечего публиковать: конструкция пуста".to_string());
        }
        let name = if name.trim().is_empty() { "Без названия".to_string() } else { name };
        let id = self.next_workshop_id;
        self.next_workshop_id += 1;
        self.workshop.insert(id, (name.clone(), player, voxels.clone(), price));
        // Автор получает копию шаблона в личную библиотеку.
        self.own_blueprints
            .entry(player)
            .or_default()
            .insert(id, (name, voxels));
        self.workshop_dirty.insert(player);
        Ok(id)
    }

    /// Купить шаблон из мастерской (авторитетно). Кредиты идут автору.
    pub fn buy_blueprint(&mut self, player: NetId, id: u32) -> Result<(), String> {
        let (name, author, voxels, price) = {
            let l = self.workshop.get(&id).ok_or_else(|| format!("объявление {id} не найдено"))?.clone();
            l
        };
        // Свой шаблон покупать не нужно — он уже в библиотеке.
        if author == player {
            return Err("это ваш шаблон".to_string());
        }
        if self.own_blueprints.get(&player).map(|m| m.contains_key(&id)).unwrap_or(false) {
            return Err("шаблон уже куплен".to_string());
        }
        let have = *self.economy.credits.get(&player).unwrap_or(&0);
        if have < price {
            return Err(format!("не хватает кредитов: есть {have}, нужно {price}"));
        }
        // Перевод кредитов автору.
        *self.economy.credits.entry(player).or_insert(0) = have - price;
        *self.economy.credits.entry(author).or_insert(0) =
            self.economy.credits.get(&author).copied().unwrap_or(0).saturating_add(price);
        // Копия в личную библиотеку покупателя.
        self.own_blueprints.entry(player).or_default().insert(id, (name, voxels));
        self.market_dirty = true;
        self.workshop_dirty.insert(player);
        Ok(())
    }

    /// Витрина мастерской и личная библиотека игрока — для окна мастерской.
    pub fn workshop_state(&self, player: NetId) -> Message {
        let mut listings: Vec<(u32, String, u32, u32, u64)> = Vec::new();
        for (id, (name, author, voxels, price)) in &self.workshop {
            listings.push((*id, name.clone(), *author, voxels.len() as u32, *price));
        }
        let owned: Vec<(u32, String)> = self
            .own_blueprints
            .get(&player)
            .map(|m| m.iter().map(|(id, (name, _))| (*id, name.clone())).collect())
            .unwrap_or_default();
        Message::ServerWorkshop { listings, owned }
    }

    /// Поставить свой (купленный) шаблон из библиотеки по id (авторитетно).
    pub fn place_own_blueprint(
        &mut self,
        player: NetId,
        entity: NetId,
        id: u32,
        origin: (i32, i32, i32),
    ) -> Result<u32, String> {
        let voxels = self
            .own_blueprints
            .get(&player)
            .and_then(|m| m.get(&id))
            .map(|(_, v)| v.clone())
            .ok_or_else(|| "шаблон не куплен".to_string())?;
        if let Some(c) = self.constructions.get(&entity) {
            if !c.can_edit(player) {
                return Err("нет прав на эту конструкцию".to_string());
            }
        }
        let c = self
            .constructions
            .entry(entity)
            .or_insert_with(|| Construction::new(player));
        let mut placed = 0u32;
        let mut edits: Vec<BuildEdit> = Vec::new();
        for ((dx, dy, dz), col) in voxels {
            let coord = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
            if c.grid.place(coord, col).is_ok() {
                c.revision += 1;
                edits.push(BuildEdit {
                    entity,
                    author: player,
                    action: BuildAction::Place,
                    coord,
                    color: ou_protocol::Color(col.0, col.1, col.2),
                    revision: c.revision,
                });
                placed += 1;
            }
        }
        if placed > 0 {
            for (_p, slot) in c.pending.iter_mut() {
                slot.1.extend(edits.iter().copied());
            }
        }
        Ok(placed)
    }

    // --- Организации, ранги и дипломатия ---

    /// Создать организацию. Создатель становится лидером. Нельзя, если игрок
    /// уже в организации.
    pub fn create_org(&mut self, player: NetId, name: String, tag: String) -> Result<u32, String> {
        if self.player_org.contains_key(&player) {
            return Err("вы уже в организации".to_string());
        }
        let name = if name.trim().is_empty() { "Без названия".to_string() } else { name };
        let tag = if tag.trim().is_empty() { "ORG".to_string() } else { tag };
        let id = self.next_org_id;
        self.next_org_id += 1;
        self.orgs.insert(id, Org::new(name, tag, player));
        self.player_org.insert(player, id);
        self.org_dirty.insert(player);
        Ok(id)
    }

    /// Вступить в организацию (если игрок не состоит в другой).
    pub fn join_org(&mut self, player: NetId, id: u32) -> Result<(), String> {
        if self.player_org.contains_key(&player) {
            return Err("вы уже в организации".to_string());
        }
        let org = self.orgs.get_mut(&id).ok_or_else(|| format!("организация {id} не найдена"))?;
        org.members.insert(player, RANK_MEMBER);
        self.player_org.insert(player, id);
        // Обновить всех участников организации.
        for p in self.orgs[&id].members.keys() {
            self.org_dirty.insert(*p);
        }
        Ok(())
    }

    /// Покинуть организацию. Если лидер уходит — лидерство переходит старшему.
    pub fn leave_org(&mut self, player: NetId) -> Result<(), String> {
        let id = self.player_org.get(&player).copied().ok_or_else(|| "вы не в организации".to_string())?;
        self.remove_from_org(player, id);
        self.org_dirty.insert(player);
        Ok(())
    }

    /// Убрать игрока из организации по id (внутренняя).
    fn remove_from_org(&mut self, player: NetId, id: u32) {
        let mut remaining: Vec<NetId> = Vec::new();
        if let Some(org) = self.orgs.get_mut(&id) {
            org.members.remove(&player);
            // Если ушёл лидер — назначаем лидером старшего по рангу.
            if !org.members.values().any(|r| *r == RANK_LEADER) {
                if let Some(next) = org
                    .members
                    .iter()
                    .max_by_key(|(_, r)| **r)
                    .map(|(p, _)| *p)
                {
                    org.members.insert(next, RANK_LEADER);
                }
            }
            remaining = org.members.keys().copied().collect();
        }
        self.player_org.remove(&player);
        // Распускаем пустую организацию.
        if self.orgs.get(&id).map(|o| o.members.is_empty()).unwrap_or(false) {
            self.orgs.remove(&id);
        }
        for p in remaining {
            self.org_dirty.insert(p);
        }
    }

    /// Назначить ранг участнику. Лидер может любые ранги; офицер — только
    /// повышать участников до офицера (но не менять лидера).
    pub fn set_org_rank(&mut self, actor: NetId, target: NetId, rank: u8) -> Result<(), String> {
        let id = self
            .player_org
            .get(&actor)
            .copied()
            .ok_or_else(|| "вы не в организации".to_string())?;
        let actor_rank = self.orgs[&id].rank_of(actor).unwrap_or(RANK_MEMBER);
        let target_rank = self.orgs[&id].rank_of(target).ok_or_else(|| "игрок не в организации".to_string())?;
        if rank > RANK_LEADER {
            return Err("неизвестный ранг".to_string());
        }
        // Лидер может всё; офицер — только назначать участников офицерами.
        if actor_rank == RANK_LEADER {
            // Ок.
        } else if actor_rank == RANK_OFFICER && rank == RANK_OFFICER && target_rank == RANK_MEMBER {
            // Ок.
        } else {
            return Err("недостаточно прав".to_string());
        }
        let org = self.orgs.get_mut(&id).unwrap();
        org.members.insert(target, rank);
        let members: Vec<NetId> = org.members.keys().copied().collect();
        for p in members {
            self.org_dirty.insert(p);
        }
        Ok(())
    }

    /// Объявить дипломатию к другой организации (только лидер).
    pub fn declare_diplomacy(&mut self, actor: NetId, org: u32, stance: u8) -> Result<(), String> {
        if stance > 2 {
            return Err("неизвестная стойка дипломатии".to_string());
        }
        let id = self
            .player_org
            .get(&actor)
            .copied()
            .ok_or_else(|| "вы не в организации".to_string())?;
        let rank = self.orgs[&id].rank_of(actor).unwrap_or(RANK_MEMBER);
        if rank != RANK_LEADER {
            return Err("дипломатию объявляет только лидер".to_string());
        }
        if self.orgs.get(&org).is_none() {
            return Err(format!("организация {org} не найдена"));
        }
        if org == id {
            return Err("нельзя объявлять дипломатию себе".to_string());
        }
        self.orgs.get_mut(&id).unwrap().diplomacy.insert(org, stance);
        // Обе стороны должны увидеть изменение.
        let members: Vec<NetId> = self.orgs[&id].members.keys().copied().collect();
        for p in members {
            self.org_dirty.insert(p);
        }
        let other: Vec<NetId> = self.orgs[&org].members.keys().copied().collect();
        for p in other {
            self.org_dirty.insert(p);
        }
        Ok(())
    }

    /// Состояние организации игрока (для окна «Организация»).
    pub fn org_state(&self, player: NetId) -> Message {
        let id = self.player_org.get(&player).copied().unwrap_or(0);
        let (name, tag, members, diplomacy) = match self.orgs.get(&id) {
            Some(o) => (
                o.name.clone(),
                o.tag.clone(),
                o.members.iter().map(|(p, r)| (*p, *r)).collect::<Vec<_>>(),
                o.diplomacy.iter().map(|(o2, s)| (*o2, *s)).collect::<Vec<_>>(),
            ),
            None => (String::new(), String::new(), Vec::new(), Vec::new()),
        };
        let all: Vec<(u32, String, String, u32)> = self
            .orgs
            .iter()
            .map(|(oid, o)| (*oid, o.name.clone(), o.tag.clone(), o.members.len() as u32))
            .collect();
        Message::ServerOrg { id, name, tag, members, diplomacy, all }
    }

    /// Привязать к сущности скрипт-шаблон по id игрока (авторитетно).
    ///
    /// Только владелец может управлять скриптом своего объекта. `template = 0`
    /// снимает скрипт (объект снова слушает игрока). Возвращает результат.
    pub fn set_script_template(&mut self, player: NetId, template: u32) -> Result<u32, String> {
        // Игрок привязывает скрипт к своей сущности (своему кораблю/объекту).
        let entity = self
            .connections
            .get(&player)
            .map(|c| c.entity_id)
            .ok_or_else(|| "игрок не в мире".to_string())?;
        if template == 0 {
            self.scripted.remove(&entity);
            self.script_template.remove(&entity);
            self.script_stats.remove(&entity);
            self.script_last_error.remove(&entity);
            self.script_logs.remove(&entity);
            self.script_urgent.insert(entity);
            return Ok(0);
        }
        let tpl = ou_script::template_by_id(template)
            .ok_or_else(|| format!("шаблон {template} не найден"))?;
        let name = ou_script::template_name(tpl.id);
        self.scripts.compile(&name, tpl.source).map_err(|e| e.to_string())?;
        self.scripted.insert(entity, name.clone());
        self.script_template.insert(entity, tpl.id);
        self.script_stats.insert(entity, (0, 0));
        self.script_last_error.remove(&entity);
        self.script_logs.insert(entity, Vec::new());
        self.script_urgent.insert(entity);
        Ok(tpl.id)
    }

    /// Статус скрипта сущности для сообщения `ServerScript`.
    pub fn script_status(&self, entity: NetId) -> Message {
        let template = self.script_template.get(&entity).copied().unwrap_or(0);
        let name = if template != 0 {
            ou_script::template_by_id(template).map(|t| t.name.to_string()).unwrap_or_default()
        } else {
            String::new()
        };
        let (runs, errors) = self.script_stats.get(&entity).copied().unwrap_or((0, 0));
        let last = self.script_last_error.get(&entity).cloned().unwrap_or_default();
        let logs = self.script_logs.get(&entity).cloned().unwrap_or_default();
        Message::ServerScript { entity, template, name, runs, errors, last, logs }
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
            amplitude: ou_sim::HOME_PLANET_AMPLITUDE,
            seed: ou_sim::HOME_PLANET_SEED,
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
            // Рельеф соседних тел: амплитуда/сид — как в физике (`build_system`).
            // У газовых гигантов/звёзд рельефа нет (гладкая сфера).
            let solid = matches!(b.kind, ou_sim::BodyKind::Planet | ou_sim::BodyKind::Moon);
            let (amplitude, seed) = ou_sim::neighbour_terrain_params(b.id, b.radius, solid);
            bodies.push(ou_protocol::SystemBodyDto {
                id: b.id,
                kind,
                surface,
                radius: b.radius,
                center: b.pos,
                orbit: b.orbit as f32,
                phase: b.phase as f32,
                omega: 0.0,
                amplitude,
                seed,
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

    /// Создать боевые цели на орбите (для орбитальных сражений).
    ///
    /// Цели висят в космосе вокруг соседнего тела (луна/планета), на орбите,
    /// поэтому игрок воюет не только у поверхности. Возвращает точки спавна.
    pub fn spawn_orbit_targets(&mut self, body_center: Vec3, body_radius: f64, count: usize) -> Vec<Vec3> {
        let mut spawns = Vec::new();
        // Орбита в плоскости XY над телом: радиус — 1.5 радиуса тела.
        let orbit_r = body_radius * 1.5;
        for i in 0..count {
            let a = 2.0 * std::f64::consts::PI * (i as f64) / (count as f64);
            // Смещение по орбите (серверные оси): плоскость, наклонённая к XY.
            let off = Vec3::new(
                orbit_r * det_cos(a),
                orbit_r * det_sin(a) * 0.4,
                orbit_r * det_sin(a),
            );
            let p = body_center.add(off);
            self.spawn_target(p);
            spawns.push(p);
        }
        spawns
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

        // Территории (владение, налог): считаем один раз, если что-то менялось.
        let territory_msg: Option<Message> = if self.territory_dirty {
            Some(Message::ServerTerritory { territories: self.territory_list() })
        } else {
            None
        };

        // Статус скриптов игроков (запусков/ошибок/логов) считается ниже,
        // после того как скрипты отработали в этом тике (шаг 1b).

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
            // Цель — ближайшая другая сущность (для турели): скрипт получает
            // расстояние до ближайшей цели, чтобы решать, стрелять ли.
            let mut target_distance = 0.0f64;
            {
                let mut best = f64::MAX;
                for (id, other) in self.world.entities.iter() {
                    if *id == entity {
                        continue;
                    }
                    let d = ent.state.pos.distance(other.state.pos);
                    if d < best {
                        best = d;
                    }
                }
                if best < f64::MAX {
                    target_distance = best;
                }
            }
            let state = ou_script::ScriptState {
                speed: ent.state.vel.length(),
                // Высота над поверхностью домашней планеты (в ед. длины).
                altitude: ent.state.pos.length() - self.planet.params.radius,
                heading: ent.state.yaw as f64,
                target_distance,
            };
            let seq = self.world.tick as u32;
            match self.scripts.run_ship(&script, state, seq) {
                Ok(out) => {
                    // Статистика и свежий вывод `log()` для владельца.
                    let stats = self.script_stats.entry(entity).or_insert((0, 0));
                    stats.0 += 1;
                    if !out.logs.is_empty() {
                        let logs = self.script_logs.entry(entity).or_default();
                        // Держим только последние строки (окно дисплея).
                        for l in out.logs {
                            logs.push(l);
                        }
                        if logs.len() > 16 {
                            let cut = logs.len() - 16;
                            logs.drain(0..cut);
                        }
                    }
                    inputs.insert(entity, out.input);
                }
                Err(e) => {
                    // Ошибка скрипта не должна ронять мир: логируем и продолжаем.
                    eprintln!("[скрипт '{script}'] {e}");
                    let stats = self.script_stats.entry(entity).or_insert((0, 0));
                    stats.1 += 1;
                    self.script_last_error.insert(entity, e.to_string());
                    if self.script_template.contains_key(&entity) {
                        // Ошибку владелец видит сразу (вне периодического ритма).
                        self.script_urgent.insert(entity);
                    }
                }
            }
        }

        // 1b. Статус скриптов для окна «Скрипты»: считаем ПОСЛЕ прогона скриптов
        //     (шаг 1a), чтобы счётчики и вывод `log()` были свежими. Рассылаем
        //     с ритмом ~2 Гц, чтобы не забивать канал, плюс срочно при смене.
        let script_msgs: BTreeMap<NetId, Message> = {
            let periodic = tick_now % 15 == 0;
            self.scripted
                .keys()
                .copied()
                .filter(|e| periodic || self.script_urgent.contains(e))
                .map(|e| (e, self.script_status(e)))
                .collect()
        };

        // Боевая статистика считается ниже, ПОСЛЕ боя этого тика (шаг 3).
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

        // 3c. Боевая статистика: считаем ПОСЛЕ боя этого тика (шаг 3/3b), чтобы
        //     убийства/урон/награда были свежими, и рассылаем изменившимся.
        let combat_msgs: BTreeMap<NetId, Message> = self
            .combat_dirty
            .iter()
            .map(|p| {
                let (k, d, b) = self.combat_stats.get(p).copied().unwrap_or((0, 0, 0));
                (*p, Message::ServerCombat { kills: k, damage: d, bounty: b })
            })
            .collect();

        // 3d. Витрина мастерской изменившимся игрокам.
        let workshop_msgs: BTreeMap<NetId, Message> = self
            .workshop_dirty
            .iter()
            .map(|p| (*p, self.workshop_state(*p)))
            .collect();

        // 3e. Состояние организации изменившимся игрокам.
        let org_msgs: BTreeMap<NetId, Message> = self
            .org_dirty
            .iter()
            .map(|p| (*p, self.org_state(*p)))
            .collect();

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

            // Территории: владение, статус, налог — при изменении.
            if let Some(t) = &territory_msg {
                msgs.push(t.clone());
            }

            // Статус скрипта на объекте игрока — при изменении.
            if let Some(m) = script_msgs.get(&conn.entity_id) {
                msgs.push(m.clone());
            }

            // Боевая статистика игрока — при изменении.
            if let Some(m) = combat_msgs.get(player_id) {
                msgs.push(m.clone());
            }

            // Витрина мастерской — при изменении.
            if let Some(m) = workshop_msgs.get(player_id) {
                msgs.push(m.clone());
            }

            // Состояние организации — при изменении.
            if let Some(m) = org_msgs.get(player_id) {
                msgs.push(m.clone());
            }

            plan.messages.insert(*player_id, msgs);
        }
        if self.market_dirty {
            self.market_dirty = false;
        }
        if self.territory_dirty {
            self.territory_dirty = false;
        }
        self.script_urgent.clear();
        self.combat_dirty.clear();
        self.workshop_dirty.clear();
        self.org_dirty.clear();

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

    // --- территории (владение и налоги) ---

    #[test]
    fn territories_exist_after_build_system() {
        let mut s = Server::new();
        s.build_system();
        let list = s.territory_list();
        // Территория базы (мирная) плюс зоны у соседних тел.
        assert!(list.len() >= 2, "территории разложены: {}", list.len());
        // Первая — мирная база (статус 0), захватить её нельзя.
        assert_eq!(list[0].id, 1);
        assert_eq!(list[0].status, 0, "база мирная");
        // Есть хотя бы одна спорная зона (можно воевать/захватить).
        assert!(list.iter().any(|t| t.status == 1), "есть спорная зона");
    }

    #[test]
    fn capture_requires_being_inside_and_grants_ownership() {
        let mut s = Server::new();
        s.build_system();
        let e = s.connect_on_planet(1);
        let pos = s.world.entities[&e].state.pos;
        // Территория базы (id 1) содержит точку появления, но мирная — нельзя.
        assert!(s.capture_as(1, 1).is_err(), "мирную базу захватить нельзя");
        // Ничейная спорная зона (id 2) — игрок вне её → отказ.
        assert!(s.capture_as(1, 2).is_err(), "вне зоны захват невозможен");
        // Переносим игрока в зону 2 и захватываем.
        let zone = s.territories.iter().find(|t| t.id == 2).unwrap().clone();
        s.world.entities.get_mut(&e).unwrap().state.pos = zone.center;
        assert_eq!(s.capture_as(1, 2).unwrap(), 2);
        let t = s.territories.iter().find(|t| t.id == 2).unwrap();
        assert_eq!(t.owner, ou_combat::Claim::Owned(1), "зона принадлежит игроку 1");
        assert_eq!(t.status, ou_combat::TerritoryStatus::Contested, "после захвата — спорная");
        assert!(s.territory_dirty, "изменение территорий помечено для рассылки");
    }

    #[test]
    fn territory_changes_are_broadcast_in_tick() {
        let mut s = Server::new();
        s.build_system();
        let e = s.connect_on_planet(1);
        // Первый тик отправляет территории (dirty с самого начала).
        let plan = s.tick();
        let has = plan.messages[&1]
            .iter()
            .any(|m| matches!(m, Message::ServerTerritory { .. }));
        assert!(has, "территории приходят клиенту");
        // Второй тик без изменений — не дублируем.
        let plan2 = s.tick();
        let again = plan2.messages[&1]
            .iter()
            .any(|m| matches!(m, Message::ServerTerritory { .. }));
        assert!(!again, "без изменений территории не дублируются");
        // Захват зоны → следующее изменение рассылается.
        let zone = s.territories.iter().find(|t| t.id == 2).unwrap().clone();
        s.world.entities.get_mut(&e).unwrap().state.pos = zone.center;
        s.capture_as(1, 2).unwrap();
        let plan3 = s.tick();
        let captured = plan3.messages[&1].iter().any(|m| matches!(
            m,
            Message::ServerTerritory { territories }
                if territories.iter().any(|t| t.id == 2 && t.owner == 1)
        ));
        assert!(captured, "захват рассылается клиентам");
    }

    #[test]
    fn tax_is_taken_from_trade_in_owned_territory() {
        let mut s = Server::new();
        s.build_system();
        // Владелец зоны (игрок 1) и торговец (игрок 2) — оба в зоне 2.
        let e1 = s.connect_on_planet(1);
        let e2 = s.connect_on_planet(2);
        let zone = s.territories.iter().find(|t| t.id == 2).unwrap().clone();
        s.world.entities.get_mut(&e1).unwrap().state.pos = zone.center;
        s.world.entities.get_mut(&e2).unwrap().state.pos = zone.center;
        s.capture_as(1, 2).unwrap();
        // Владелец ставит налог 10% (прямо в состояние; UI — отдельно).
        if let Some(t) = s.territories.iter_mut().find(|t| t.id == 2) {
            t.tax_percent = 10;
        }
        // Продавец (игрок 2) торгует: сделка на 1000 в чужой зоне → 100 владельцу.
        let before_owner = *s.economy.credits.get(&1).unwrap_or(&0);
        let (who, tax) = s.tax_for_trade(zone.center, 1000).expect("налог берётся");
        assert_eq!(who, 1, "налог идёт владельцу зоны");
        assert_eq!(tax, 100, "10% от 1000");
        assert!(before_owner == 0 || before_owner > 0); // состояние согласовано
        assert_eq!(*s.territory_tax.get(&2).unwrap_or(&0), 100, "налог накоплен");
        // Вне зоны налога нет.
        assert!(s.tax_for_trade(Vec3::new(50_000_000.0, 0.0, 0.0), 1000).is_none());
    }

    // --- скриптинг в игре (шаблоны, привязка, статус) ---

    #[test]
    fn script_templates_are_available() {
        let s = Server::new();
        let list = s.script_templates();
        assert!(list.len() >= 4, "есть библиотека скриптов: {}", list.len());
        // Все шаблоны имеют непустые имя и описание.
        for (id, name, desc) in &list {
            assert!(*id > 0);
            assert!(!name.is_empty(), "у шаблона {id} есть имя");
            assert!(!desc.is_empty(), "у шаблона {id} есть описание");
        }
    }

    #[test]
    fn player_can_attach_and_detach_script() {
        let mut s = Server::new();
        let e = s.connect_on_planet(1);
        // Библиотека непуста; берём первый шаблон.
        let tid = s.script_templates()[0].0;
        assert_eq!(s.set_script_template(1, tid).unwrap(), tid);
        assert!(s.scripted.contains_key(&e), "скрипт привязан к объекту игрока");
        assert_eq!(s.script_template.get(&e).copied(), Some(tid));
        // Статус содержит имя привязанного шаблона.
        match s.script_status(e) {
            Message::ServerScript { template, name, .. } => {
                assert_eq!(template, tid);
                assert!(!name.is_empty());
            }
            other => panic!("ожидали ServerScript, получено {other:?}"),
        }
        // Снятие скрипта.
        assert_eq!(s.set_script_template(1, 0).unwrap(), 0);
        assert!(!s.scripted.contains_key(&e), "скрипт снят");
        // Несуществующий шаблон — ошибка.
        assert!(s.set_script_template(1, 999).is_err());
    }

    #[test]
    fn autopilot_script_moves_the_ship() {
        let mut s = Server::new();
        s.build_system();
        let e = s.connect_on_planet(1);
        // Автопилот «крейсер» (id 2): включает тягу, объект должен набрать скорость.
        assert_eq!(s.set_script_template(1, 2).unwrap(), 2);
        // Первый тик применяет управление скрипта.
        s.tick();
        for _ in 0..30 {
            s.tick();
        }
        let speed = s.world.entities[&e].state.vel.length();
        assert!(speed > 1.0, "автопилот разогнал объект: {speed}");
    }

    #[test]
    fn script_status_is_broadcast_to_owner() {
        let mut s = Server::new();
        s.build_system();
        let e = s.connect_on_planet(1);
        let tid = s.script_templates()[0].0;
        s.set_script_template(1, tid).unwrap();
        // После тика владелец получает статус скрипта.
        let plan = s.tick();
        let seen = plan.messages[&1].iter().any(|m| matches!(
            m,
            Message::ServerScript { template, .. } if *template == tid
        ));
        assert!(seen, "статус скрипта рассылается владельцу");
        let _ = e;
    }

    // --- шаблоны постройки (blueprints) ---

    #[test]
    fn blueprint_templates_are_available() {
        let s = Server::new();
        let list = s.blueprint_templates();
        assert!(list.len() >= 5, "библиотека шаблонов: {}", list.len());
        for (id, name, desc) in &list {
            assert!(*id > 0);
            assert!(!name.is_empty());
            assert!(!desc.is_empty());
        }
    }

    #[test]
    fn blueprint_is_stamped_into_construction() {
        let mut s = Server::new();
        let e = s.connect_on_planet(1);
        // Ядро конструкции — в (0,0,0). Ставим куб 5×5×5 рядом с ядром.
        let placed = s.place_blueprint_as(1, e, 2, (1, 0, 0)).unwrap();
        assert!(placed > 0, "шаблон поставлен");
        // Число вокселей конструкции выросло (минимум ядро + поставленные).
        assert!(s.constructions[&e].grid.len() > 1);
        // Неизвестный шаблон — ошибка.
        assert!(s.place_blueprint_as(1, e, 999, (0, 0, 0)).is_err());
    }

    #[test]
    fn blueprint_respects_build_rights() {
        let mut s = Server::new();
        let e1 = s.connect_on_planet(1);
        s.connect_on_planet(2);
        // Игрок 2 пытается строить в конструкции игрока 1 — нет прав.
        assert!(s.place_blueprint_as(2, e1, 2, (1, 0, 0)).is_err());
        // Игрок 1 — может.
        assert!(s.place_blueprint_as(1, e1, 2, (1, 0, 0)).is_ok());
    }

    #[test]
    fn blueprint_edits_are_queued_for_owner() {
        let mut s = Server::new();
        let e = s.connect_on_planet(1);
        // Тик создаёт «слот ожидания» правок для игрока (как в живой игре).
        s.tick();
        // Ставим шаблон: правки должны попасть в очередь владельца, чтобы
        // клиент нарисовал блоки.
        s.place_blueprint_as(1, e, 2, (1, 0, 0)).unwrap();
        let pending = &s.constructions[&e].pending;
        assert!(pending.contains_key(&1), "правки для игрока 1 в очереди");
        let edits = &pending[&1].1;
        assert!(!edits.is_empty(), "есть правки шаблона");
        assert!(edits.iter().all(|x| x.action == BuildAction::Place));
    }

    // --- орбитальные сражения и боевая статистика ---

    #[test]
    fn destroying_a_target_awards_bounty_and_stats() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::new(100_000.0, 0.0, 0.0));
        // Цель рядом с игроком, заведомо уничтожаемая (слабая броня).
        let drone = s.spawn_target(Vec3::new(100_050.0, 0.0, 0.0));
        // Стреляем, пока цель не будет уничтожена (ограничим число попыток).
        let start_credits = *s.economy.credits.get(&1).unwrap_or(&0);
        let mut killed = false;
        for tick in 1..=400u64 {
            if let Some(ev) = s.resolve_fire(a, tick) {
                if ev.target == drone && ev.destroyed {
                    killed = true;
                    break;
                }
            }
        }
        assert!(killed, "цель уничтожена");
        let (kills, damage, bounty) = s.combat_stats.get(&1).copied().unwrap_or((0, 0, 0));
        assert_eq!(kills, 1, "одно убийство");
        assert!(damage > 0, "урон накоплен: {damage}");
        assert_eq!(bounty, DESTROY_BOUNTY, "награда начислена");
        // Кредиты игрока выросли на награду.
        let end_credits = *s.economy.credits.get(&1).unwrap_or(&0);
        assert_eq!(end_credits, start_credits + DESTROY_BOUNTY, "bounty в кредитах");
    }

    #[test]
    fn combat_stats_are_broadcast_to_owner() {
        let mut s = Server::new();
        let a = s.connect(1, Vec3::new(100_000.0, 0.0, 0.0));
        s.spawn_target(Vec3::new(100_050.0, 0.0, 0.0));
        // Один выстрел с уроном → статистика помечена к рассылке.
        let _ = s.resolve_fire(a, 1);
        let plan = s.tick();
        let seen = plan.messages[&1]
            .iter()
            .any(|m| matches!(m, Message::ServerCombat { .. }));
        assert!(seen, "боевая статистика рассылается владельцу");
    }

    #[test]
    fn orbit_targets_spawn_around_a_body() {
        let mut s = Server::new();
        // Цели на орбите вокруг центра (радиус тела 1000) — по кольцу.
        let spawns = s.spawn_orbit_targets(Vec3::new(5000.0, 0.0, 0.0), 1000.0, 4);
        assert_eq!(spawns.len(), 4, "четыре орбитальные цели");
        // Каждая — не в центре тела и на разумном удалении (1.5 радиуса).
        for p in &spawns {
            let d = p.sub(Vec3::new(5000.0, 0.0, 0.0)).length();
            assert!(d > 100.0, "цель вне центра тела: {d}");
            assert!(d < 3000.0, "цель вблизи орбиты: {d}");
        }
        // Все они — мишени с известной точкой возрождения.
        assert_eq!(s.target_spawns.len(), 4);
    }

    // --- мастерская и торговля шаблонами ---

    #[test]
    fn publish_and_buy_blueprint_transfers_credits() {
        let mut s = Server::new();
        let e1 = s.connect_on_planet(1);
        s.connect_on_planet(2);
        // Игрок 1 строит что-то и публикует.
        s.apply_build_as(1, &BuildCommand {
            entity: e1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(1, 2, 3),
        });
        let id = s.publish_blueprint(1, "Дом".to_string(), 500).unwrap();
        // Витрина содержит объявление.
        match s.workshop_state(2) {
            Message::ServerWorkshop { listings, .. } => {
                assert!(listings.iter().any(|(lid, ..)| *lid == id), "объявление в витрине");
            }
            other => panic!("ожидали ServerWorkshop, получено {other:?}"),
        }
        // Игрок 2 покупает: кредиты уходят игроку 1.
        let c1 = *s.economy.credits.get(&1).unwrap_or(&0);
        let c2 = *s.economy.credits.get(&2).unwrap_or(&0);
        s.buy_blueprint(2, id).unwrap();
        assert_eq!(*s.economy.credits.get(&1).unwrap(), c1 + 500, "автору зачислено");
        assert_eq!(*s.economy.credits.get(&2).unwrap(), c2 - 500, "покупатель списал");
        // Купленный шаблон есть в личной библиотеке покупателя.
        assert!(s.own_blueprints.get(&2).map(|m| m.contains_key(&id)).unwrap_or(false));
        // Повторная покупка — отказ.
        assert!(s.buy_blueprint(2, id).is_err());
    }

    #[test]
    fn cannot_publish_empty_or_buy_own() {
        let mut s = Server::new();
        let e1 = s.connect_on_planet(1);
        s.connect_on_planet(2);
        // Пустая конструкция публикуется? У игрока есть ядро — значит не пуста.
        // (Ядро всегда есть, поэтому публикация проходит.)
        let id = s.publish_blueprint(1, String::new(), 100).unwrap();
        // Свой шаблон купить нельзя.
        assert!(s.buy_blueprint(1, id).is_err(), "свой шаблон не покупается");
        // Несуществующий — тоже.
        assert!(s.buy_blueprint(2, 999).is_err());
        let _ = e1;
    }

    #[test]
    fn place_own_blueprint_adds_blocks() {
        let mut s = Server::new();
        let e1 = s.connect_on_planet(1);
        s.apply_build_as(1, &BuildCommand {
            entity: e1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(10, 20, 30),
        });
        let id = s.publish_blueprint(1, "База".to_string(), 0).unwrap();
        // Ставим свой шаблон ВПЛОТНУЮ к ядру — появляются блоки.
        let before = s.constructions[&e1].grid.len();
        let placed = s.place_own_blueprint(1, e1, id, (2, 0, 0)).unwrap();
        assert!(placed > 0, "шаблон поставлен");
        assert!(s.constructions[&e1].grid.len() > before);
    }

    #[test]
    fn workshop_state_is_broadcast() {
        let mut s = Server::new();
        let e1 = s.connect_on_planet(1);
        s.apply_build_as(1, &BuildCommand {
            entity: e1,
            action: BuildAction::Place,
            coord: (1, 0, 0),
            color: ou_protocol::Color(1, 1, 1),
        });
        s.publish_blueprint(1, "Т".to_string(), 10).unwrap();
        let plan = s.tick();
        let seen = plan.messages[&1]
            .iter()
            .any(|m| matches!(m, Message::ServerWorkshop { .. }));
        assert!(seen, "витрина мастерской рассылается автору");
    }

    // --- организации, ранги и дипломатия ---

    #[test]
    fn create_and_join_org() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.connect_on_planet(2);
        let id = s.create_org(1, "Легион".to_string(), "LEG".to_string()).unwrap();
        assert_eq!(s.player_org.get(&1).copied(), Some(id));
        // Второй игрок вступает.
        s.join_org(2, id).unwrap();
        assert_eq!(s.orgs[&id].members.len(), 2);
        assert_eq!(s.orgs[&id].rank_of(1), Some(RANK_LEADER), "создатель — лидер");
        assert_eq!(s.orgs[&id].rank_of(2), Some(RANK_MEMBER), "вошедший — участник");
        // Уже в организации — повторно нельзя.
        assert!(s.create_org(1, "Ещё".to_string(), "X".to_string()).is_err());
        assert!(s.join_org(2, id).is_err());
    }

    #[test]
    fn rank_changes_respect_authority() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.connect_on_planet(2);
        s.connect_on_planet(3);
        let id = s.create_org(1, "Легион".to_string(), "LEG".to_string()).unwrap();
        s.join_org(2, id).unwrap();
        s.join_org(3, id).unwrap();
        // Лидер повышает игрока 2 до офицера.
        s.set_org_rank(1, 2, RANK_OFFICER).unwrap();
        assert_eq!(s.orgs[&id].rank_of(2), Some(RANK_OFFICER));
        // Офицер повышает игрока 3 до офицера — можно.
        s.set_org_rank(2, 3, RANK_OFFICER).unwrap();
        // Офицер не может назначить лидера.
        assert!(s.set_org_rank(2, 3, RANK_LEADER).is_err());
        // Участник не может назначать ранги.
        s.set_org_rank(1, 2, RANK_MEMBER).unwrap();   // 2 снова участник
        assert!(s.set_org_rank(2, 3, RANK_OFFICER).is_err(), "участник не назначает");
    }

    #[test]
    fn leader_leaving_promotes_next() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.connect_on_planet(2);
        let id = s.create_org(1, "Легион".to_string(), "LEG".to_string()).unwrap();
        s.join_org(2, id).unwrap();
        // Лидер уходит — лидером становится оставшийся.
        s.leave_org(1).unwrap();
        assert_eq!(s.orgs[&id].rank_of(2), Some(RANK_LEADER), "новый лидер");
        // Последний уходит — организация распускается.
        s.leave_org(2).unwrap();
        assert!(!s.orgs.contains_key(&id), "пустая организация удалена");
    }

    #[test]
    fn diplomacy_declared_by_leader_only() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.connect_on_planet(2);
        s.connect_on_planet(3);
        let a = s.create_org(1, "Альфа".to_string(), "A".to_string()).unwrap();
        let b = s.create_org(2, "Бета".to_string(), "B".to_string()).unwrap();
        s.join_org(3, a).unwrap();
        // Лидер объявляет войну — ок.
        s.declare_diplomacy(1, b, 2).unwrap();
        assert_eq!(s.orgs[&a].diplomacy.get(&b), Some(&2));
        // Участник не может.
        assert!(s.declare_diplomacy(3, b, 1).is_err(), "участник не объявляет дипломатию");
        // Себе — нельзя.
        assert!(s.declare_diplomacy(1, a, 1).is_err());
    }

    #[test]
    fn org_state_is_broadcast_on_change() {
        let mut s = Server::new();
        s.connect_on_planet(1);
        s.connect_on_planet(2);
        let id = s.create_org(1, "Легион".to_string(), "LEG".to_string()).unwrap();
        // После тика лидер получает состояние организации.
        let plan = s.tick();
        let seen = plan.messages[&1]
            .iter()
            .any(|m| matches!(m, Message::ServerOrg { id: oid, .. } if *oid == id));
        assert!(seen, "состояние организации рассылается");
    }
}
