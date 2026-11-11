//! Демонстрация шардирования мира по звёздным системам.

use ou_shard::{Routing, ShardMap, ShardRouter, TransferState};
use ou_sim::{generate_system, Vec3};
use std::collections::BTreeMap;

fn main() {
    println!("Open Universe — шардирование мира (consistent hashing)\n");

    // Кластер из 8 звёздных систем.
    let count = 8u32;
    let spacing = 250_000.0;
    let mut router = ShardRouter::new(spacing / 2.0);
    for i in 0..count {
        let sys = generate_system(i + 1, 0xABCD + i as u64 * 7919, Vec3::new(i as f64 * spacing, 0.0, 0.0));
        println!("  зона {}: {:<12} центр x = {:>10.0}", sys.id, sys.name, sys.pos.x);
        router.register_system(&sys);
    }

    // Сначала 2 узла.
    println!("\n=== Кластер из 2 узлов ===");
    router.map = ShardMap::with_nodes(&[1, 2]);
    print_distribution(&router, count);

    // Добавляем третий узел — смотрим, сколько зон переехало.
    let owners_before: Vec<_> = (1..=count).map(|z| router.owner_of(z)).collect();
    println!("\n=== Добавляем узел 3 (масштабирование) ===");
    router.map.add_node(3);
    let owners_after: Vec<_> = (1..=count).map(|z| router.owner_of(z)).collect();
    let moved = owners_before
        .iter()
        .zip(&owners_after)
        .filter(|(a, b)| a != b)
        .count();
    println!("Переехало зон: {moved} из {count} (consistent hashing двигает минимум)");
    print_distribution(&router, count);

    // Игрок летит через весь кластер: показываем миграции.
    println!("\n=== Полёт игрока через весь кластер (старт на узле 1) ===");
    let entity = TransferState {
        player_id: 42,
        entity_id: 7,
        pos: Vec3::ZERO,
        vel: Vec3::new(0.0, 0.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        last_input_seq: 100,
    };
    // Важно: после миграции игрок УЖЕ на новом узле — иначе миграция повторяется.
    let mut current_node = 1u32;
    let mut hops = 0;
    let steps = 40;
    for i in 0..=steps {
        let x = i as f64 / steps as f64 * (count as f64 - 0.5) * spacing;
        let pos = Vec3::new(x, 0.0, 0.0);
        match router.route(pos, current_node) {
            Routing::Local => {}
            Routing::Migrate { to } => {
                hops += 1;
                println!(
                    "  x = {:>10.0} -> миграция с узла {current_node} на узел {to}",
                    x
                );
                current_node = to; // игрок переехал
            }
            Routing::Unassigned => {}
        }
    }
    println!("Всего миграций за полёт: {hops} (узел назначения в конце: {current_node})");
    println!("Перенос состояния игрока #{} (тик ввода {})", entity.entity_id, entity.last_input_seq);
}

fn print_distribution(router: &ShardRouter, count: u32) {
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    for z in 1..=count {
        if let Some(n) = router.owner_of(z) {
            *counts.entry(n).or_default() += 1;
        }
    }
    for (node, c) in &counts {
        println!("  узел {node}: {c} зон");
    }
}
