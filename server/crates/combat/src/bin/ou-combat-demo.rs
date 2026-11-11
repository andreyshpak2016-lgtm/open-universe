//! Демонстрация боевых систем: попадания, щит, броня, бой и территории.

use ou_combat::{
    apply_outcome, resolve_shot, shot_hits, weapon, Attack, Battle, Claim, ShipHull, Territory,
    TerritoryStatus,
};
use ou_protocol::Vec3;

fn main() {
    println!("Open Universe — боевые системы\n");

    // --- Вероятность попадания от дистанции ---
    println!("=== Шанс попадания (1000 выстрелов) ===");
    for name in ["автопушка", "рельсотрон", "торпеда"] {
        let w = weapon(name).unwrap();
        print!("  {name:<12} дальность {:>5.0}: ", w.range);
        for frac in [0.0, 0.25, 0.5, 0.75, 0.95] {
            let d = w.range * frac;
            let hits = (0..1000u64).filter(|s| shot_hits(&w, d, *s)).count();
            print!("{:>3}% ", hits / 10);
        }
        println!();
    }

    // --- Дуэль: две пушки против щита и брони ---
    println!("\n=== Дуэль: лазер (энергия) против рельсотрона (кинетика) ===");
    let laser = weapon("лазер").unwrap();
    let rail = weapon("рельсотрон").unwrap();
    let mut hull = ShipHull::new(300, 400);
    println!(
        "  цель: щит {}/{} ({:>3}%), броня {}/{} ({:>3}%)",
        hull.shield, 300, hull.shield_percent(), hull.armor, 400, hull.armor_percent()
    );

    let mut battle = Battle::new();
    let distance = 900.0;

    for tick in 1..=40u64 {
        // Попеременно бьют лазер и рельсотрон.
        let (w, attacker) = if tick % 2 == 0 { (&laser, 1u32) } else { (&rail, 2u32) };
        let seed = tick.wrapping_mul(0x9E37) ^ attacker as u64;
        let o = resolve_shot(&hull, w, distance, seed);
        apply_outcome(&mut hull, &o);
        battle.record(Attack { attacker, target: 9, weapon_index: 0, tick, outcome: o });

        if tick % 8 == 0 || o.destroyed {
            println!(
                "  тик {:>2}: щит {:>3} ({:>3}%), броня {:>3} ({:>3}%){}",
                tick,
                hull.shield,
                hull.shield_percent(),
                hull.armor,
                hull.armor_percent(),
                if o.destroyed { "  ← уничтожен" } else { "" }
            );
        }
        if hull.is_destroyed() {
            break;
        }
        // Щит медленно восстанавливается между залпами.
        if tick % 10 == 0 {
            hull.recharge(3);
        }
    }

    println!("\nИтог боя: нанесено урона {}", battle.total_damage());
    println!("  игрок 1 (лазер): {}", battle.damage_of(1));
    println!("  игрок 2 (рельсотрон): {}", battle.damage_of(2));
    if let Some(w) = battle.winner() {
        println!("  победитель по урону: игрок {w}");
    }
    println!("  цель уничтожена: {}", hull.is_destroyed());

    // --- Территории ---
    println!("\n=== Территории ===");
    let mut mine = Territory::new(1, 1, Vec3::ZERO, 5000.0);
    mine.owner = Claim::Owned(7);
    mine.tax_percent = 5;

    let mut contested = Territory::new(2, 1, Vec3::new(50_000.0, 0.0, 0.0), 5000.0);
    contested.status = TerritoryStatus::Contested;

    let mut fortress = Territory::new(3, 1, Vec3::new(100_000.0, 0.0, 0.0), 5000.0);
    fortress.status = TerritoryStatus::Fortified;
    fortress.control_shield = 80;

    for t in [&mine, &contested, &fortress] {
        println!(
            "  территория {}: статус {:?}, щит контроля {:>3}, урон проходит {:>3}%, налог {}%",
            t.id,
            t.status,
            t.control_shield,
            t.damage_scale_percent(),
            t.tax_percent
        );
    }
    println!("  бой в мирной зоне разрешён: {}", mine.allows_combat());
    println!("  бой в спорной зоне разрешён: {}", contested.allows_combat());
    println!("  налог с сделки 1000 кр: {} кр", mine.tax_on(1000));

    // Захват территории.
    contested.capture(7, 1200);
    println!(
        "  после захвата: владелец {:?}, статус {:?}, с тика {}",
        contested.owner, contested.status, contested.held_since_tick
    );

    println!("\nВсе атаки в журнале: {}", battle.log.len());
}
