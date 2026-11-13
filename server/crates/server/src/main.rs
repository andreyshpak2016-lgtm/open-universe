//! Демонстрационный запуск авторитетного сервера без сети.
//!
//! Два игрока стартуют вне зоны видимости друг друга и сближаются. Показываем
//! работу interest management: клиент накапливает известный набор сущностей из
//! базовой линии и последующих дельт, и в какой-то момент игрок 2 «появляется».

use ou_protocol::{InputFrame, Message, Snapshot, Vec3};
use ou_server::{Server, INTEREST_RADIUS};
use std::collections::BTreeSet;

/// Клиентский «вид»: накапливает сущности из baseline и дельт.
#[derive(Default)]
struct View {
    known: BTreeSet<u32>,
}

impl View {
    fn apply(&mut self, snap: &Snapshot) {
        if snap.baseline {
            self.known.clear();
        }
        for e in &snap.entities {
            self.known.insert(e.id);
        }
    }
}

fn main() {
    println!("Open Universe — сервер единого шарда (демо tick loop)");
    println!("Тик: 30 Гц | радиус интереса: {INTEREST_RADIUS} ед.");
    println!("Игрок 1 стартует в 0, игрок 2 — в {:.0} и летит навстречу.\n", INTEREST_RADIUS * 1.6);

    let mut server = Server::new();
    server.connect(1, Vec3::ZERO);
    // Игрок 2 стартует на оси −Z и летит «вперёд» (+Z) прямо к центру.
    server.connect(2, Vec3::new(0.0, 0.0, -INTEREST_RADIUS * 1.6));

    let mut view1 = View::default();
    let mut total_bytes = 0usize;

    println!("{:>5} | {:>9} | {:>9} | {:>8} | {}", "tick", "видит", "id", "байт", "дист.");
    println!("{}", "-".repeat(58));

    for tick in 1..=300u32 {
        // Игрок 1 стоит на месте и ждёт. Игрок 2 летит к нему «вперёд».
        server.receive_input(
            2,
            &[InputFrame { seq: tick, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false, ..Default::default() }],
        );

        let plan = server.tick();

        // Применяем дельты клиента 1.
        if let Some(msgs) = plan.messages.get(&1) {
            for m in msgs {
                match m {
                    Message::ServerSnapshot(s) => {
                        view1.apply(s);
                        // Эмулируем измерение размера снапшота по проводу.
                        let snap_bytes = ou_protocol::encode(&Message::ServerSnapshot(s.clone()));
                        total_bytes += snap_bytes.len();
                    }
                    _ => {}
                }
            }
        }

        if tick % 20 == 0 {
            let bytes_here = plan
                .messages
                .get(&1)
                .map(|ms| {
                    ms.iter()
                        .map(|m| match m {
                            Message::ServerSnapshot(s) => {
                                ou_protocol::encode(&Message::ServerSnapshot(s.clone())).len()
                            }
                            other => ou_protocol::encode(other).len(),
                        })
                        .sum()
                })
                .unwrap_or(0);
            let dist = server
                .world
                .entities
                .get(&1)
                .zip(server.world.entities.get(&2))
                .map(|(a, b)| a.state.pos.distance(b.state.pos))
                .unwrap_or(0.0);
            println!(
                "{:>5} | {:>9} | {:>9} | {:>8} | {:.0}",
                tick,
                view1.known.len(),
                view1.known.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","),
                bytes_here,
                dist
            );
        }
    }

    println!("\nИтого переданно клиенту 1: {} байт за 300 тиков (10 с)", total_bytes);
    println!("Средний трафик: {:.0} байт/с на игрока", total_bytes as f64 / 10.0);
    println!("Контрольная сумма мира: {:#x}", server.world.checksum());
}
