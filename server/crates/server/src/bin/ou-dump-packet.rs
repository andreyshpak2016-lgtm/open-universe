//! Дамп реального сетевого пакета сервера в файл.
//!
//! Зачем: чтобы проверить, что **C++-клиент** (каркас UE5) умеет разбирать
//! пакеты, которые формирует Rust-сервер. Мы прогоняем несколько тиков
//! симуляции, берём снапшот, кодируем его тем же протоколом и кладём в файл
//! (и рядом — ожидаемые значения для сверки).
//!
//! Запуск:
//!   cargo run -p ou-server --bin ou-dump-packet -- <файл.dat> [ticks]

use ou_protocol::{Message, Vec3};
use ou_server::Server;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "packet.dat".to_string());
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);

    let token = 0x376085675394cb58u64;

    // Готовим мир: три корабля с разными курсами.
    let mut server = Server::new();
    server.connect(1, Vec3::ZERO);
    server.connect(2, Vec3::new(120.0, 0.0, -80.0));
    server.connect(3, Vec3::new(-60.0, 30.0, 150.0));

    for t in 0..ticks {
        for (id, yaw) in [(1u32, 0.15f32), (2, -0.3), (3, 0.05)] {
            server.receive_input(
                id,
                &[ou_protocol::InputFrame {
                    seq: t,
                    throttle: 1.0,
                    yaw,
                    pitch: 0.05,
                    fire: false,
                }],
            );
        }
        server.tick();
    }

    // Берём снимок, который сервер отправил бы игроку 1 (baseline: все видимые).
    let plan = server.tick();
    let msgs = plan.messages.get(&1).expect("есть сообщение для игрока 1");
    let snap_msg = msgs
        .iter()
        .find(|m| matches!(m, Message::ServerSnapshot(_)))
        .expect("есть снапшот")
        .clone();

    let Message::ServerSnapshot(snap) = &snap_msg else {
        unreachable!()
    };

    // Кодируем пакет «как на проводе»: [token u64][len u32][body].
    let body = ou_protocol::encode(&snap_msg);
    let mut packet: Vec<u8> = Vec::with_capacity(12 + body.len());
    packet.extend_from_slice(&token.to_le_bytes());
    packet.extend_from_slice(&(body.len() as u32).to_le_bytes());
    packet.extend_from_slice(&body);

    std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(&packet))
        .expect("запись пакета");

    println!("Пакет сохранён: {path} ({} байт)", packet.len());
    println!("Тик: {}, baseline: {}, сущностей: {}", snap.tick, snap.baseline, snap.entities.len());
    println!("\nОжидаемые значения (эталон для сверки с C++):");
    for e in &snap.entities {
        // e.pos уже в игровых единицах (Vec3::from_wire поделил на масштаб).
        // Осталось перевести игровые единицы в сантиметры UE: 1 единица = 1 м = 100 см.
        let cm = 100.0;
        println!(
            "  id {:<3} pos ({:>10.2}, {:>10.2}, {:>10.2})  yaw {:.4}",
            e.id,
            e.pos.x * cm,
            e.pos.y * cm,
            e.pos.z * cm,
            e.yaw
        );
    }
}
