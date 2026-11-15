//! # Модули корабля: груз, топливо, энергия.
//!
//! Это «бортовые системы» сущности: сколько места в трюме, сколько топлива в
//! баках и сколько энергии в сети. Ёмкости **растут от модулей**, которые игрок
//! строит и держит в инвентаре (контейнер, топливный бак, генератор), — так
//! экономика (крафт модулей) напрямую влияет на полёт.
//!
//! ## Детерминизм
//! Только целые числа и базовые операции: топливо и энергия воспроизводимы
//! бит-в-бит. Кинематику они не меняют (иначе разошлось бы предсказание
//! клиента): топливо и энергия ограничивают **действия** — буст и огонь.

use ou_economy::{Category, ItemRegistry};

/// Базовое место в трюме без модулей (единиц грузового отсека).
pub const BASE_CARGO: u64 = 200;
/// Сколько места добавляет один контейнер.
pub const CARGO_PER_CONTAINER: u64 = 400;
/// Базовый объём бака без модулей (единиц топлива).
pub const BASE_FUEL: u64 = 20_000;
/// Сколько топлива добавляет один бак.
pub const FUEL_PER_TANK: u64 = 40_000;
/// Базовая ёмкость энергосети без модулей.
pub const BASE_ENERGY: u64 = 5_000;
/// Сколько энергии добавляет один генератор.
pub const ENERGY_PER_GENERATOR: u64 = 10_000;

/// Расход топлива за единицу тяги в секунду (тяга и буст считаются отдельно).
///
/// Баланс: базовый бак (20 000) при полной тяге тратится ~330 с (5.5 мин), а в
/// сверхсвете — ~17 с непрерывного полёта (перелёт между телами за секунды).
pub const FUEL_PER_THRUST: u64 = 2;
/// Расход топлива в сверхсвете за тик (множитель к тяге).
pub const FUEL_BOOST_MUL: u64 = 20;
/// Стоимость одного выстрела в энергии.
pub const ENERGY_PER_SHOT: u64 = 120;
/// Сколько энергии в секунду даёт базовая сеть (рекуперация).
pub const ENERGY_REGEN: u64 = 60;
/// Сколько топлива даёт одна единица предмета «топливо» при заправке.
pub const FUEL_PER_ITEM: u64 = 2_000;

/// Бортовые системы одной сущности.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ShipSystems {
    /// Топливо в баках.
    pub fuel: u64,
    /// Максимум топлива (от модулей).
    pub fuel_max: u64,
    /// Энергия в сети.
    pub energy: u64,
    /// Максимум энергии (от модулей).
    pub energy_max: u64,
    /// Занято места в трюме.
    pub cargo_used: u64,
    /// Ёмкость трюма (от модулей).
    pub cargo_max: u64,
}

impl ShipSystems {
    /// Создать системы с полными баками и ёмкостями из модулей.
    pub fn new(fuel_max: u64, energy_max: u64, cargo_max: u64) -> Self {
        ShipSystems {
            fuel: fuel_max,
            fuel_max,
            energy: energy_max,
            energy_max,
            cargo_used: 0,
            cargo_max,
        }
    }

    /// Доля топлива 0..1 (для HUD).
    pub fn fuel_ratio(&self) -> f32 {
        if self.fuel_max == 0 {
            0.0
        } else {
            self.fuel as f32 / self.fuel_max as f32
        }
    }

    /// Доля энергии 0..1 (для HUD).
    pub fn energy_ratio(&self) -> f32 {
        if self.energy_max == 0 {
            0.0
        } else {
            self.energy as f32 / self.energy_max as f32
        }
    }

    /// Есть ли топливо для сверхсвета.
    pub fn can_boost(&self) -> bool {
        self.fuel > 0
    }

    /// Есть ли энергия на выстрел.
    pub fn can_fire(&self) -> bool {
        self.energy >= ENERGY_PER_SHOT
    }
}

/// Ёмкости и «топливоёмкость» по набору модулей (счётчики из инвентаря).
///
/// `containers` — сколько контейнеров, `tanks` — баков, `generators` —
/// генераторов. Возвращает (макс. топливо, макс. энергия, ёмкость трюма).
pub fn capacities_from_modules(containers: u64, tanks: u64, generators: u64) -> (u64, u64, u64) {
    let fuel_max = BASE_FUEL.saturating_add(tanks.saturating_mul(FUEL_PER_TANK));
    let energy_max = BASE_ENERGY.saturating_add(generators.saturating_mul(ENERGY_PER_GENERATOR));
    let cargo_max = BASE_CARGO.saturating_add(containers.saturating_mul(CARGO_PER_CONTAINER));
    (fuel_max, energy_max, cargo_max)
}

/// Сколько в инвентаре предметов-модулей нужной категории, по именам.
///
/// Модули — это предметы категории «конструкция» с именами «контейнер»,
/// «топливный бак», «генератор». Считаем их в инвентаре владельца (в любой
/// системе — модуль стоит на корабле, а корабль один).
pub struct ModuleCounts {
    pub containers: u64,
    pub tanks: u64,
    pub generators: u64,
}

impl ModuleCounts {
    /// Посчитать модули в инвентаре игрока по реестру предметов.
    pub fn from_inventory(
        inv: &ou_economy::Inventory,
        owner: ou_economy::OwnerId,
        reg: &ItemRegistry,
    ) -> Self {
        let mut counts = ModuleCounts { containers: 0, tanks: 0, generators: 0 };
        for ((o, _sys), bag) in inv.iter_slots() {
            if *o != owner {
                continue;
            }
            for (item, qty) in bag {
                let Some(def) = reg.get(*item) else { continue };
                // Только модули-конструкции (их и крафтят по рецептам).
                if def.category != Category::Construct {
                    continue;
                }
                match def.name.as_str() {
                    "контейнер" => counts.containers += qty,
                    "топливный бак" => counts.tanks += qty,
                    "генератор" => counts.generators += qty,
                    _ => {}
                }
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_increase_capacities() {
        let (f0, e0, c0) = capacities_from_modules(0, 0, 0);
        assert_eq!((f0, e0, c0), (BASE_FUEL, BASE_ENERGY, BASE_CARGO));
        let (f1, e1, c1) = capacities_from_modules(2, 3, 1);
        assert_eq!(f1, BASE_FUEL + 3 * FUEL_PER_TANK);
        assert_eq!(e1, BASE_ENERGY + 1 * ENERGY_PER_GENERATOR);
        assert_eq!(c1, BASE_CARGO + 2 * CARGO_PER_CONTAINER);
    }

    #[test]
    fn fuel_and_energy_gate_actions() {
        let mut s = ShipSystems::new(1000, 500, 200);
        assert!(s.can_boost());
        assert!(s.can_fire());
        s.fuel = 0;
        assert!(!s.can_boost(), "без топлива сверхсвет недоступен");
        s.energy = ENERGY_PER_SHOT - 1;
        assert!(!s.can_fire(), "без энергии стрелять нельзя");
        s.energy = ENERGY_PER_SHOT;
        assert!(s.can_fire());
    }

    #[test]
    fn ratios_are_safe_when_empty() {
        let s = ShipSystems::new(0, 0, 0);
        assert_eq!(s.fuel_ratio(), 0.0);
        assert_eq!(s.energy_ratio(), 0.0);
    }
}
