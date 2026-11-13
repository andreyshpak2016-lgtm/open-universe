//! Демонстрация персистентности мира: сохранение и восстановление после
//! «перезапуска» сервера.
//!
//! Показывает, что единый шард переживает рестарт: состояние сущностей, тик и
//! нумерация новых сущностей сохраняются.

use ou_persist::{to_json, FileStore, Store, WorldSnapshot};
use ou_protocol::{InputFrame, Vec3};
use ou_server::Server;
use std::collections::BTreeMap;

fn main() {
    let dir = std::env::temp_dir().join("ou-world-demo");
    println!("Open Universe — персистентность мира");
    println!("Каталог хранилища: {}\n", dir.display());

    let mut store = FileStore::open(&dir).expect("хранилище");
    // Чистая демонстрация: начинаем с пустого слота.
    let _ = std::fs::remove_file(dir.join("world.snapshot"));
    let _ = std::fs::remove_file(dir.join("world.snapshot.prev"));

    // ---------- Сессия 1 ----------
    println!("=== Сессия 1: сервер запущен, игроки летают ===");
    let mut server = Server::new();
    let e1 = server.connect(1, Vec3::ZERO);
    let e2 = server.connect(2, Vec3::new(0.0, 0.0, -1500.0));
    println!("созданы сущности: {e1}, {e2}");

    for tick in 1..=120u32 {
        server.receive_input(
            1,
            &[InputFrame { seq: tick, throttle: 1.0, yaw: 0.1, pitch: 0.0, fire: false, ..Default::default() }],
        );
        server.receive_input(
            2,
            &[InputFrame { seq: tick, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false, ..Default::default() }],
        );
        server.tick();
    }

    let before = server.world.checksum();
    let pos_before: Vec<_> = server
        .world
        .entities
        .iter()
        .map(|(id, e)| (*id, e.state.pos))
        .collect();
    for (id, p) in &pos_before {
        println!("  сущность {id}: ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
    }
    println!("тик: {} | контрольная сумма: {before:#x}", server.world.tick);

    let bytes = ou_persist::save_world(&mut store, &server.world).expect("сохранение");
    println!("сохранено в снимок: {bytes} байт\n");

    // ---------- «Перезапуск» ----------
    println!("=== Перезапуск сервера (процесс завершён) ===");
    drop(server);

    // Новый процесс: читаем мир с диска.
    let store2 = FileStore::open(&dir).expect("хранилище");
    let mut server2 = Server::new();
    let loaded = ou_persist::load_world(&store2).expect("загрузка");
    println!(
        "загружено сущностей: {} | тик: {}",
        loaded.entities.len(),
        loaded.tick
    );
    let after = loaded.checksum();
    println!("контрольная сумма после загрузки: {after:#x}");
    println!("состояние совпало -> {}\n", before == after);

    server2.adopt_world(loaded);
    // Новый игрок должен получить свободный id, не конфликтующий с миром.
    let e3 = server2.connect(3, Vec3::new(500.0, 0.0, 500.0));
    println!("=== Новый игрок после загрузки ===");
    println!("новая сущность получила id = {e3} (конфликтов нет: id > {})", e3 - 1);

    for tick in 1..=30u32 {
        server2.receive_input(
            1,
            &[InputFrame { seq: tick, throttle: 0.5, yaw: -0.2, pitch: 0.0, fire: false, ..Default::default() }],
        );
        server2.tick();
    }
    let pos_after: BTreeMap<u32, Vec3> = server2
        .world
        .entities
        .iter()
        .map(|(id, e)| (*id, e.state.pos))
        .collect();
    println!("после 30 тиков движения:");
    for (id, p) in &pos_after {
        println!("  сущность {id}: ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
    }

    // ---------- Демонстрация форматов ----------
    let snap = WorldSnapshot::capture(&server2.world);
    println!("\n=== Хранилище ===");
    println!("бинарный снимок: {} байт", snap.to_bytes().len());
    let json = to_json(&snap);
    println!("JSON-экспорт: {} байт (для отладки)", json.len());
    match store2.load() {
        Ok(Some(s)) => println!("снимок на диске содержит {} сущностей", s.entities.len()),
        Ok(None) => println!("снимок на диске отсутствует"),
        Err(e) => println!("ошибка чтения снимка: {e}"),
    }

    // Оставляем каталог как артефакт демонстрации.
    println!("\nСнимок мира оставлен в: {}", dir.display());
}
