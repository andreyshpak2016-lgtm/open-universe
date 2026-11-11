//! Демонстрация детерминированной генерации звёздных систем.

use ou_sim::{generate_system, BodyKind, Vec3};
use std::collections::BTreeMap;

fn kind_ru(k: BodyKind) -> &'static str {
    match k {
        BodyKind::Star => "звезда",
        BodyKind::Planet => "планета",
        BodyKind::Moon => "луна",
        BodyKind::GasGiant => "газовый гигант",
        BodyKind::AsteroidBelt => "пояс астероидов",
        BodyKind::BlackHole => "чёрная дыра",
    }
}

fn main() {
    println!("Open Universe — генерация мира (детерминированная)\n");

    // Генерируем кластер из 6 систем на детерминированной сетке.
    let mut systems = BTreeMap::new();
    let spacing = 250_000.0;
    for i in 0..6u32 {
        let seed = 0xC0FFEE + i as u64 * 7919;
        let center = Vec3::new(i as f64 * spacing, 0.0, 0.0);
        systems.insert(i, generate_system(i + 1, seed, center));
    }

    let mut total_bodies = 0;
    let mut total_resources: u64 = 0;
    let mut black_holes = 0;

    for (_, s) in &systems {
        let star = s.body(0).unwrap();
        let planets = s.bodies.iter().filter(|b| matches!(b.kind, BodyKind::Planet | BodyKind::GasGiant)).count();
        let moons = s.bodies.iter().filter(|b| b.kind == BodyKind::Moon).count();
        let belts = s.bodies.iter().filter(|b| b.kind == BodyKind::AsteroidBelt).count();
        if star.kind == BodyKind::BlackHole {
            black_holes += 1;
        }
        total_bodies += s.bodies.len();
        total_resources += s.total_resources();

        println!(
            "{:<14} центр: {:<10} | тел: {:>2} (планет {:>2}, лун {:>2}, поясов {:>2}) | ресурсов: {:>6}",
            s.name,
            kind_ru(star.kind),
            s.bodies.len(),
            planets,
            moons,
            belts,
            s.total_resources()
        );
    }

    println!("\nИтого: {} систем, {} тел, {} чёрных дыр", systems.len(), total_bodies, black_holes);
    println!("Суммарные ресурсы кластера: {total_resources}");

    // Демонстрация орбитального движения: следим за поясом астероидов.
    let s = systems.get(&0).unwrap();
    if let Some(belt) = s.bodies.iter().find(|b| b.kind == BodyKind::AsteroidBelt) {
        println!("\nОрбита пояса астероидов #{} в системе {}:", belt.id, s.name);
        for t in [0.0, 250.0, 500.0, 750.0] {
            let p = s.body_position_at(belt.id, t).unwrap();
            println!("  t={:>6.0} c -> x={:>12.1} z={:>12.1}", t, p.x, p.z);
        }
    }

    // Проверка детерминизма на уровне демонстрации.
    let again = generate_system(1, 0xC0FFEE, Vec3::ZERO);
    let first = generate_system(1, 0xC0FFEE, Vec3::ZERO);
    println!("\nПовторная генерация идентична -> {}", again == first);
}
