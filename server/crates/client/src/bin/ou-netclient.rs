//! Open Universe — клиент по реальному UDP с предсказанием и реконсиляцией.
//!
//! Запуск: `cargo run -p ou-client --bin ou-netclient -- [адрес] [секунды]`
//! По умолчанию: `127.0.0.1:7777`, 4 секунды.

use ou_client::PredictedClient;
use ou_net::{new_session_token, UdpTransport};
use ou_protocol::{InputFrame, Message, NetId, TICK_DT};
use ou_sim::det_sin;
use std::net::UdpSocket;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:7777".to_string());
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);

    let token = new_session_token(0x5EED);
    let sock = UdpSocket::bind("127.0.0.1:0").expect("не удалось занять локальный порт");
    let mut net = UdpTransport::from_socket(sock).expect("транспорт");
    net.set_nonblocking(true).expect("неблокирующий режим");
    let server_addr: std::net::SocketAddr = addr.parse().expect("неверный адрес сервера");

    println!("ou-netclient -> {server_addr} (токен {token:#018x})");

    net.send_to(token, server_addr, &Message::ClientHello { protocol: 1, name: "pilot".into() })
        .expect("отправка hello");
    net.flush().expect("flush hello");

    let mut me: Option<NetId> = None;
    let mut client: Option<PredictedClient> = None;
    let mut seq: u32 = 0;
    let mut snapshots: u64 = 0;
    let mut reconciles: u64 = 0;
    let mut max_correction: f64 = 0.0;
    let mut total_correction: f64 = 0.0;

    let start = Instant::now();
    let frame_dt = Duration::from_secs_f64(TICK_DT);

    while start.elapsed().as_secs_f64() < seconds {
        let loop_start = Instant::now();

        // --- приём ---
        loop {
            match net.recv_message(token) {
                Ok(Some((_from, msg))) => match msg {
                    Message::ServerSnapshot(snap) => {
                        snapshots += 1;
                        let id = *me.get_or_insert_with(|| {
                            let first = snap.entities.first().map(|e| e.id).unwrap_or(1);
                            println!("baseline получен, моя сущность = {first}");
                            first
                        });
                        // На клиенте хранится ровно одна сущность — его корабль.
                        match client.as_mut() {
                            None => {
                                // Первый снимок: клиент встаёт на авторитетную точку
                                // без предсказания — это инициализация, не расхождение.
                                let mut c = PredictedClient::new(id);
                                if let Some(e) = snap.entities.iter().find(|e| e.id == id) {
                                    if let Some(ent) = c.world.entities.get_mut(&id) {
                                        ent.state = *e;
                                    }
                                }
                                client = Some(c);
                            }
                            Some(c) => {
                                let before = c.world.entities.get(&id).map(|e| e.state.pos);
                                c.reconcile(&snap.entities, id, snap.ack_input_seq);
                                reconciles += 1;
                                if let (Some(b), Some(a)) =
                                    (before, c.world.entities.get(&id).map(|e| e.state.pos))
                                {
                                    let d = b.distance(a);
                                    total_correction += d;
                                    if d > max_correction {
                                        max_correction = d;
                                    }
                                }
                            }
                        }
                    }
                    Message::ServerAck { last_input_seq, .. } => {
                        if let Some(c) = client.as_mut() {
                            c.ack(last_input_seq);
                        }
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // --- ввод и предсказание ---
        if let (Some(id), Some(c)) = (me, client.as_mut()) {
            seq += 1;
            let input = InputFrame {
                seq,
                throttle: 1.0,
                yaw: (det_sin(seq as f64 * 0.05) * 0.4) as f32,
                pitch: 0.0,
                fire: false,
            };
            c.local_input(input, id); // применяем немедленно (предсказание)
            net.send_to(token, server_addr, &Message::ClientInput { inputs: vec![input] }).ok();
            net.flush().ok();
        }

        let elapsed = loop_start.elapsed();
        if elapsed < frame_dt {
            sleep(frame_dt - elapsed);
        }
    }

    println!("\n--- итоги клиента ---");
    println!("кадров отправлено: {seq}");
    println!("снимков принято: {snapshots}");
    println!("реконсиляций: {reconciles}");
    println!("суммарная коррекция: {total_correction:.3}");
    println!("максимальная коррекция: {max_correction:.3}");
    if let (Some(id), Some(c)) = (me, client.as_ref()) {
        if let Some(e) = c.world.entities.get(&id) {
            println!(
                "предсказанная позиция: ({:.1}, {:.1}, {:.1})",
                e.state.pos.x, e.state.pos.y, e.state.pos.z
            );
        }
        println!("неподтверждённых вводов: {}", c.pending.len());
    }
}
