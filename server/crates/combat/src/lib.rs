//! # ou-combat
//!
//! Боевые системы Open Universe: оружие, щиты, броня, повреждения по секциям
//! и территории.
//!
//! ## Детерминизм
//! Никаких `rand`: попадание считается по чистой функции от позиций, времени и
//! «зерна выстрела». Состояние корабля — целые числа (проценты брони и щита),
//! поэтому бой воспроизводим бит-в-бит, как физика и экономика.
//!
//! ## Что здесь есть
//! * [`Weapon`] — оружие с уроном, дальностью, перезарядкой и разбросом.
//! * [`ShipHull`] — секции корпуса: щит, броня, критические узлы.
//! * [`resolve_shot`] — один выстрел: попадание, поглощение щитом, урон броне.
//! * [`Territory`] — зона с владельцем, щитом контроля и налогом.
//! * [`Battle`] — учёт урона и определение победителя по накопленному урону.

use ou_protocol::Vec3;

// ---------------------------------------------------------------------------
// Оружие
// ---------------------------------------------------------------------------

/// Тип урона.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    /// Кинетический: хорошо пробивает броню, слабее против щита.
    Kinetic,
    /// Энергетический: сильно бьёт по щиту, слабее по броне.
    Energy,
    /// Взрывной: большой урон, медленная перезарядка.
    Explosive,
}

/// Оружие.
#[derive(Clone, Debug, PartialEq)]
pub struct Weapon {
    pub name: String,
    /// Базовый урон за выстрел.
    pub damage: u32,
    pub kind: DamageKind,
    /// Дальность действия.
    pub range: f64,
    /// Перезарядка в тиках.
    pub cooldown: u64,
    /// Разброс: чем больше, тем ниже шанс попадания на пределе дальности.
    /// Задан целым числом «промахов на километр» — без дробной случайности.
    pub spread_milli: u32,
}

impl Weapon {
    pub fn new(name: &str, damage: u32, kind: DamageKind, range: f64, cooldown: u64) -> Self {
        Weapon { name: name.to_string(), damage, kind, range, cooldown, spread_milli: 20 }
    }

    pub fn with_spread(mut self, spread_milli: u32) -> Self {
        self.spread_milli = spread_milli;
        self
    }
}

/// Стандартные орудия Open Universe.
pub fn weapon(name: &str) -> Option<Weapon> {
    match name {
        "автопушка" => Some(Weapon::new("автопушка", 12, DamageKind::Kinetic, 1500.0, 4)),
        "лазер" => Some(Weapon::new("лазер", 9, DamageKind::Energy, 2200.0, 6)),
        "рельсотрон" => Some(Weapon::new("рельсотрон", 45, DamageKind::Kinetic, 4000.0, 40)
            .with_spread(5)),
        "торпеда" => Some(Weapon::new("торпеда", 90, DamageKind::Explosive, 6000.0, 90)
            .with_spread(40)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Щит и броня
// ---------------------------------------------------------------------------

/// Состояние корпуса корабля.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipHull {
    /// Текущий щит.
    pub shield: u32,
    maximum_shield: u32,
    /// Текущая броня.
    pub armor: u32,
    maximum_armor: u32,
    /// Ущерб критическому узлу (0..100). При 100 корабль уничтожен.
    pub core_damage: u32,
}

/// Результат одного выстрела.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotOutcome {
    pub hit: bool,
    /// Пробило ли щит и дошло ли до брони.
    pub breached_shield: bool,
    pub shield_damage: u32,
    pub armor_damage: u32,
    pub core_damage: u32,
    /// Корабль уничтожен этим выстрелом.
    pub destroyed: bool,
}

impl ShipHull {
    pub fn new(shield: u32, armor: u32) -> Self {
        ShipHull {
            shield,
            maximum_shield: shield,
            armor,
            maximum_armor: armor,
            core_damage: 0,
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.armor == 0 || self.core_damage >= 100
    }

    /// Мгновенно уничтожить корабль (для тестов и правил).
    pub fn destroy(&mut self) {
        self.armor = 0;
        self.shield = 0;
        self.core_damage = 100;
    }

    /// Регенерация щита за `ticks` тиков (детерминированная).
    pub fn recharge(&mut self, ticks: u64) {
        if self.is_destroyed() {
            return;
        }
        // 1 единица щита за тик, не выше максимума.
        let gained = (ticks as u32).min(self.maximum_shield - self.shield.min(self.maximum_shield));
        self.shield += gained;
    }

    pub fn shield_percent(&self) -> u32 {
        if self.maximum_shield == 0 {
            0
        } else {
            self.shield * 100 / self.maximum_shield
        }
    }

    pub fn armor_percent(&self) -> u32 {
        if self.maximum_armor == 0 {
            0
        } else {
            self.armor * 100 / self.maximum_armor
        }
    }
}

/// Попадание: детерминированное, без случайных чисел.
///
/// Шанс попадания зависит от дистанции и разброса оружия. «Случайность»
/// берётся из `shot_seed` — целого числа, которое сервер ведёт сам (тик + id
/// стрелка + номер выстрела), поэтому результат воспроизводим.
pub fn shot_hits(weapon: &Weapon, distance: f64, shot_seed: u64) -> bool {
    if distance > weapon.range {
        return false;
    }
    // 100% в упор, линейно падает к пределу дальности.
    let closeness = 1.0 - (distance / weapon.range);
    // Базовый шанс: 30% + 70% по близости, минус штраф разброса.
    let spread_penalty = weapon.spread_milli as f64 / 1000.0;
    let chance = (30.0 + 70.0 * closeness) / 100.0 - spread_penalty;
    // Равномерная величина [0,1) из зерна: хеш-функция даёт ровное
    // распределение, в отличие от прямой подстановки в тригонометрию.
    let roll = hash_unit(shot_seed);
    roll < chance.clamp(0.0, 1.0)
}

/// Детерминированное значение [0,1) из целого зерна (SplitMix64 + FNV-смесь).
///
/// Тригонометрию для этого не используем: `det_sin(seed*k)` даёт заметно
/// неравномерное распределение, из-за чего шансы попадания «плывут».
pub fn hash_unit(seed: u64) -> f64 {
    let mut z = seed ^ 0x9E3779B97F4A7C15;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

// ---------------------------------------------------------------------------
// Разрешение выстрела
// ---------------------------------------------------------------------------

/// Множители урона по типу.
fn shield_factor(kind: DamageKind) -> f64 {
    match kind {
        DamageKind::Kinetic => 0.6,
        DamageKind::Energy => 1.5,
        DamageKind::Explosive => 1.0,
    }
}

fn armor_factor(kind: DamageKind) -> f64 {
    match kind {
        DamageKind::Kinetic => 1.3,
        DamageKind::Energy => 0.8,
        DamageKind::Explosive => 1.6,
    }
}

/// Разрешить один выстрел: проверка попадания, поглощение щитом, урон броне.
///
/// Возвращает исход, ничего не мутируя — состояние применяет вызывающая сторона
/// через [`apply_outcome`], чтобы можно было откатывать в тестах.
pub fn resolve_shot(
    target: &ShipHull,
    weapon: &Weapon,
    distance: f64,
    shot_seed: u64,
) -> ShotOutcome {
    if target.is_destroyed() {
        return ShotOutcome {
            hit: false,
            breached_shield: false,
            shield_damage: 0,
            armor_damage: 0,
            core_damage: 0,
            destroyed: true,
        };
    }
    if !shot_hits(weapon, distance, shot_seed) {
        return ShotOutcome {
            hit: false,
            breached_shield: false,
            shield_damage: 0,
            armor_damage: 0,
            core_damage: 0,
            destroyed: false,
        };
    }

    let raw = weapon.damage;
    // Сначала щит, затем броня.
    let shield_dmg_float = raw as f64 * shield_factor(weapon.kind);
    let to_shield = shield_dmg_float.round().min(target.shield as f64) as u32;
    let leftover = raw.saturating_sub(to_shield);
    let armor_dmg = (leftover as f64 * armor_factor(weapon.kind)).round() as u32;
    let armor_damage = armor_dmg.min(target.armor);
    let penetrated = armor_dmg > target.armor; // пробило насквозь

    // Критический узел получает урон только при пробитии брони.
    let core_damage = if penetrated {
        (armor_dmg - target.armor).min(100)
    } else {
        0
    };

    let armor_left = target.armor.saturating_sub(armor_damage);
    let core_after = (target.core_damage + core_damage).min(100);
    let destroyed = armor_left == 0 || core_after >= 100;

    ShotOutcome {
        hit: true,
        breached_shield: to_shield == target.shield && target.shield > 0,
        shield_damage: to_shield,
        armor_damage,
        core_damage,
        destroyed,
    }
}

/// Применить исход выстрела к корпусу.
pub fn apply_outcome(hull: &mut ShipHull, o: &ShotOutcome) {
    hull.shield = hull.shield.saturating_sub(o.shield_damage);
    hull.armor = hull.armor.saturating_sub(o.armor_damage);
    hull.core_damage = (hull.core_damage + o.core_damage).min(100);
}

// ---------------------------------------------------------------------------
// Территории
// ---------------------------------------------------------------------------

/// Владелец территории.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Ничья.
    Neutral,
    /// Принадлежит владельцу с указанным id.
    Owned(u32),
}

/// Статус территории.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerritoryStatus {
    /// Спокойная зона: бой запрещён.
    Peaceful,
    /// Спорная: можно воевать, идёт захват.
    Contested,
    /// Укреплённая: щит контроля снижает урон до прорыва.
    Fortified,
}

/// Территория (зона) в звёздной системе.
#[derive(Clone, Debug, PartialEq)]
pub struct Territory {
    pub id: u32,
    pub system: u32,
    pub center: Vec3,
    pub radius: f64,
    pub owner: Claim,
    pub status: TerritoryStatus,
    /// Щит контроля (0..100): пока цел, урон по постройкам снижен.
    pub control_shield: u32,
    /// Налог с торговли, в процентах (0..25).
    pub tax_percent: u32,
    /// Тик, с которого территория удерживается текущим владельцем.
    pub held_since_tick: u64,
}

impl Territory {
    pub fn new(id: u32, system: u32, center: Vec3, radius: f64) -> Self {
        Territory {
            id,
            system,
            center,
            radius,
            owner: Claim::Neutral,
            status: TerritoryStatus::Peaceful,
            control_shield: 100,
            tax_percent: 0,
            held_since_tick: 0,
        }
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.distance(self.center) <= self.radius
    }

    /// Множитель урона по постройкам с учётом щита контроля (проценты).
    ///
    /// Возвращает, сколько процентов урона **проходит** до цели. Чем крепче
    /// щит контроля, тем меньше проходит урона.
    pub fn damage_scale_percent(&self) -> u32 {
        match self.status {
            TerritoryStatus::Peaceful => 0, // бой запрещён
            TerritoryStatus::Contested => 50,
            TerritoryStatus::Fortified => {
                // Целый щит (100) пропускает 20%, без щита проходит 100%.
                (100 - self.control_shield.min(100) * 80 / 100).max(20)
            }
        }
    }

    /// Можно ли здесь вообще стрелять.
    pub fn allows_combat(&self) -> bool {
        !matches!(self.status, TerritoryStatus::Peaceful)
    }

    /// Захват: владелец меняется, статус — «спорная».
    pub fn capture(&mut self, new_owner: u32, tick: u64) {
        self.owner = Claim::Owned(new_owner);
        self.status = TerritoryStatus::Contested;
        self.held_since_tick = tick;
    }

    /// Налог с суммы сделки (целые числа).
    pub fn tax_on(&self, amount: u64) -> u64 {
        amount * self.tax_percent.min(25) as u64 / 100
    }
}

// ---------------------------------------------------------------------------
// Бой
// ---------------------------------------------------------------------------

/// Запись об атаке для статистики и античита.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attack {
    pub attacker: u32,
    pub target: u32,
    pub weapon_index: usize,
    pub tick: u64,
    pub outcome: ShotOutcome,
}

/// Бой: счётчик урона по участникам.
#[derive(Clone, Debug, Default)]
pub struct Battle {
    /// Накопленный урон, нанесённый каждым участником.
    damage_dealt: std::collections::BTreeMap<u32, u64>,
    /// Журнал атак (ограничен по размеру на практике).
    pub log: Vec<Attack>,
}

impl Battle {
    pub fn new() -> Self {
        Battle { damage_dealt: std::collections::BTreeMap::new(), log: Vec::new() }
    }

    /// Зарегистрировать атаку и накопить урон.
    pub fn record(&mut self, a: Attack) {
        let total = (a.outcome.shield_damage + a.outcome.armor_damage + a.outcome.core_damage) as u64;
        *self.damage_dealt.entry(a.attacker).or_insert(0) += total;
        self.log.push(a);
    }

    pub fn damage_of(&self, id: u32) -> u64 {
        self.damage_dealt.get(&id).copied().unwrap_or(0)
    }

    /// Победитель — по накопленному урону (детерминированно при равенстве: меньший id).
    pub fn winner(&self) -> Option<u32> {
        self.damage_dealt
            .iter()
            .min_by_key(|(id, dmg)| (std::cmp::Reverse(**dmg), **id))
            .map(|(id, _)| *id)
    }

    /// Общий урон в бою.
    pub fn total_damage(&self) -> u64 {
        self.damage_dealt.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shot_at_point_blank_almost_always_hits() {
        let w = weapon("автопушка").unwrap();
        let mut hits = 0;
        for seed in 0..100u64 {
            if shot_hits(&w, 0.0, seed) {
                hits += 1;
            }
        }
        assert!(hits > 95, "в упор должно попадать почти всегда, попало {hits}");
    }

    #[test]
    fn shot_beyond_range_never_hits() {
        let w = weapon("автопушка").unwrap();
        for seed in 0..100u64 {
            assert!(!shot_hits(&w, w.range + 1.0, seed));
        }
    }

    #[test]
    fn hit_chance_decreases_with_distance() {
        let w = weapon("лазер").unwrap();
        let count = |d: f64| (0..1000u64).filter(|s| shot_hits(&w, d, *s)).count();
        let near = count(w.range * 0.1);
        let far = count(w.range * 0.9);
        assert!(near > far, "вблизи попаданий должно быть больше: {near} vs {far}");
    }

    #[test]
    fn hit_is_deterministic_for_same_seed() {
        let w = weapon("рельсотрон").unwrap();
        for seed in 0..50u64 {
            assert_eq!(shot_hits(&w, 1000.0, seed), shot_hits(&w, 1000.0, seed));
        }
    }

    #[test]
    fn energy_weapon_hits_shield_harder() {
        let laser = weapon("лазер").unwrap();
        let hull = ShipHull::new(100, 100);
        // Ищем зерно, при котором выстрел попадает.
        let seed = (0..100u64).find(|s| shot_hits(&laser, 100.0, *s)).unwrap();
        let o = resolve_shot(&hull, &laser, 100.0, seed);
        assert!(o.hit);
        // Энергетический урон по щиту — с множителем 1.5.
        assert!(o.shield_damage > laser.damage, "лазер должен бить по щиту сильнее базового");
    }

    #[test]
    fn kinetic_weapon_hits_armor_harder() {
        let rail = weapon("рельсотрон").unwrap();
        // Цели без щита: весь урон идёт в броню.
        let hull = ShipHull::new(0, 1000);
        let seed = (0..200u64).find(|s| shot_hits(&rail, 100.0, *s)).unwrap();
        let o = resolve_shot(&hull, &rail, 100.0, seed);
        assert!(o.hit);
        assert!(o.armor_damage > rail.damage, "кинетика должна бить по броне сильнее");
    }

    #[test]
    fn shield_absorbs_before_armor() {
        let laser = weapon("лазер").unwrap();
        let hull = ShipHull::new(1000, 500);
        let seed = (0..100u64).find(|s| shot_hits(&laser, 50.0, *s)).unwrap();
        let o = resolve_shot(&hull, &laser, 50.0, seed);
        assert!(o.shield_damage > 0);
        assert_eq!(o.armor_damage, 0, "при целом щите урон до брони не доходит");
    }

    #[test]
    fn damage_overwhelms_shield_and_reaches_armor() {
        let torpedo = weapon("торпеда").unwrap();
        let hull = ShipHull::new(10, 500); // почти нет щита
        let seed = (0..500u64).find(|s| shot_hits(&torpedo, 100.0, *s)).unwrap();
        let o = resolve_shot(&hull, &torpedo, 100.0, seed);
        assert!(o.shield_damage > 0);
        assert!(o.armor_damage > 0, "избыток урона должен пройти в броню");
        assert!(o.breached_shield);
    }

    #[test]
    fn applying_damage_reduces_hull_and_can_destroy() {
        let rail = weapon("рельсотрон").unwrap();
        let mut hull = ShipHull::new(0, 100);
        let mut seed = 0u64;
        let mut fired = 0;
        while !hull.is_destroyed() && fired < 1000 {
            if shot_hits(&rail, 50.0, seed) {
                let o = resolve_shot(&hull, &rail, 50.0, seed);
                apply_outcome(&mut hull, &o);
                fired += 1;
            }
            seed += 1;
        }
        assert!(hull.is_destroyed(), "достаточное число попаданий должно уничтожить корабль");
    }

    #[test]
    fn destroyed_ship_takes_no_more_damage() {
        let mut hull = ShipHull::new(100, 100);
        hull.destroy();
        let torpedo = weapon("торпеда").unwrap();
        let o = resolve_shot(&hull, &torpedo, 10.0, 1);
        assert!(!o.hit);
        assert!(o.destroyed);
    }

    #[test]
    fn shield_recharge_is_bounded_and_skips_dead() {
        let mut hull = ShipHull::new(100, 100);
        // Опустошим щит.
        let o = ShotOutcome {
            hit: true,
            breached_shield: true,
            shield_damage: 100,
            armor_damage: 0,
            core_damage: 0,
            destroyed: false,
        };
        apply_outcome(&mut hull, &o);
        assert_eq!(hull.shield, 0);
        hull.recharge(30);
        assert_eq!(hull.shield, 30);
        hull.recharge(1000);
        assert_eq!(hull.shield, 100, "щит не превышает максимум");

        let mut dead = ShipHull::new(100, 100);
        dead.destroy();
        dead.recharge(50);
        assert_eq!(dead.shield, 0, "уничтоженный корабль не заряжается");
    }

    #[test]
    fn territory_peaceful_forbids_combat() {
        let t = Territory::new(1, 1, Vec3::ZERO, 1000.0);
        assert!(!t.allows_combat());
        assert_eq!(t.damage_scale_percent(), 0);
    }

    #[test]
    fn territory_fortified_reduces_damage() {
        let mut t = Territory::new(1, 1, Vec3::ZERO, 1000.0);
        t.status = TerritoryStatus::Fortified;
        t.control_shield = 100;
        assert_eq!(t.damage_scale_percent(), 20, "целый щит пропускает лишь 20% урона");
        t.control_shield = 0;
        assert_eq!(t.damage_scale_percent(), 100, "без щита проходит весь урон");
        t.control_shield = 50;
        assert_eq!(t.damage_scale_percent(), 60, "щит наполовину ослабляет урон");
    }

    #[test]
    fn territory_contains_and_capture() {
        let mut t = Territory::new(1, 1, Vec3::new(1000.0, 0.0, 0.0), 500.0);
        assert!(t.contains(Vec3::new(1200.0, 0.0, 0.0)));
        assert!(!t.contains(Vec3::ZERO));
        t.capture(7, 42);
        assert_eq!(t.owner, Claim::Owned(7));
        assert_eq!(t.status, TerritoryStatus::Contested);
        assert_eq!(t.held_since_tick, 42);
    }

    #[test]
    fn territory_tax_is_capped() {
        let mut t = Territory::new(1, 1, Vec3::ZERO, 100.0);
        t.tax_percent = 10;
        assert_eq!(t.tax_on(1000), 100);
        t.tax_percent = 99; // выше максимума
        assert_eq!(t.tax_on(1000), 250, "налог ограничен 25%");
    }

    #[test]
    fn battle_tracks_damage_and_winner() {
        let weapon = weapon("автопушка").unwrap();
        let hull = ShipHull::new(0, 1000);
        let seed = (0..100u64).find(|s| shot_hits(&weapon, 100.0, *s)).unwrap();
        let outcome = resolve_shot(&hull, &weapon, 100.0, seed);

        let mut battle = Battle::new();
        battle.record(Attack { attacker: 1, target: 2, weapon_index: 0, tick: 1, outcome });
        battle.record(Attack { attacker: 2, target: 1, weapon_index: 0, tick: 2, outcome });
        battle.record(Attack { attacker: 1, target: 2, weapon_index: 0, tick: 3, outcome });

        assert!(battle.damage_of(1) > battle.damage_of(2));
        assert_eq!(battle.winner(), Some(1));
        assert_eq!(battle.log.len(), 3);
        assert!(battle.total_damage() > 0);
    }

    #[test]
    fn combat_is_deterministic() {
        // Одинаковые входы → одинаковый исход, как и требуется для MMO.
        let run = || {
            let w = weapon("лазер").unwrap();
            let mut hull = ShipHull::new(200, 300);
            let mut total = 0u32;
            for tick in 0..50u64 {
                let o = resolve_shot(&hull, &w, 800.0, tick * 7 + 1);
                apply_outcome(&mut hull, &o);
                total += o.shield_damage + o.armor_damage + o.core_damage;
            }
            (hull, total)
        };
        assert_eq!(run(), run());
    }
}
