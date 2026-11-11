//! Open Universe — авторитетный сервер по реальному UDP.
//!
//! Запуск: `cargo run -p ou-server --bin ou-netd -- [адрес] [секунды]`
//! По умолчанию: `127.0.0.1:7777`, 4 секунды.
//!
//! Что делает:
//! 1. Принимает `ClientHello`, регистрирует игрока и запоминает его адрес.
//! 2. Принимает `ClientInput` и передаёт их в детерминированную симуляцию.
//! 3. Каждый тик (30 Гц) рассылает дельта-снимки с учётом зон интереса.

use ou_net::{new_session_token, UdpTransport};
use ou_protocol::{InputFrame, Message, NetId, Snapshot, Vec3, TICK_DT};
use ou_server::{Server, INTEREST_RADIUS};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:7777".to_string());
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);

    let token = new_session_token(0x5EED);
    let mut net = UdpTransport::bind(&addr).expect("не удалось занять UDP-порт");
    println!("ou-netd: слушаю {}", net.local_addr().unwrap());
    println!("токен сессии: {token:#018x} | тик 30 Гц | радиус интереса {INTEREST_RADIUS}");
    println!("работаю {seconds} с...\n");

    let mut server = Server::new();
    // addr -> (player_id, token)
    let mut clients: BTreeMap<SocketAddr, NetId> = BTreeMap::new();
    let mut next_player: NetId = 1;
    let mut next_spawn = 0.0;

    let tick_dt = Duration::from_secs_f64(TICK_DT);
    let start = Instant::now();
    let mut ticks: u64 = 0;
    let mut bytes_out: u64 = 0;
    let mut snapshots_out: u64 = 0;

    while start.elapsed().as_secs_f64() < seconds {
        let loop_start = Instant::now();

        // --- приём ---
        loop {
            match net.recv_message(token) {
                Ok(Some((from, msg))) => match msg {
                    Message::ClientHello { name, .. } => {
                        if !clients.contains_key(&from) {
                            // Разносим точки спавна по оси −Z, чтобы игроки сближались.
                            let spawn = Vec3::new(0.0, 0.0, -2000.0 - next_spawn);
                            next_spawn += 400.0;
                            let entity = server.connect(next_player, spawn);
                            clients.insert(from, next_player);
                            println!(
                                "игрок {next_player} «{name}» вошёл с {from} (сущность {entity})"
                            );
                            next_player += 1;
                        }
                    }
                    Message::ClientInput { inputs } => {
                        if let Some(pid) = clients.get(&from) {
                            server.receive_input(*pid, &inputs);
                        }
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // --- шаг симуляции и рассылка ---
        let plan = server.tick();
        for (player_id, msgs) in &plan.messages {
            // Найдём адрес игрока по id.
            if let Some((addr, _)) = clients.iter().find(|(_, pid)| *pid == player_id) {
                for m in msgs {
                    if let Ok(()) = net.send_to(token, *addr, m) {
                        bytes_out += ou_net::encode_packet(token, m).len() as u64;
                        if matches!(m, Message::ServerSnapshot(_)) {
                            snapshots_out += 1;
                        }
                    }
                }
            }
        }
        net.flush().ok();
        ticks += 1;

        // --- выдержка частоты ---
        let elapsed = loop_start.elapsed();
        if elapsed < tick_dt {
            sleep(tick_dt - elapsed);
        }
    }

    println!("\n--- итоги ---");
    println!("тиков симуляции: {ticks}");
    println!("игроков обслужено: {}", clients.len());
    println!("снапшотов отправлено: {snapshots_out}");
    println!("байт отправлено: {bytes_out}");
    if ticks > 0 {
        println!("средний исходящий трафик: {:.0} байт/тик", bytes_out as f64 / ticks as f64);
    }
    println!("контрольная сумма мира: {:#x}", server.world.checksum());

    // Для наглядности — где оказались игроки.
    for (addr, pid) in &clients {
        if let Some(ent) = server.world.entities.iter().find(|(_, c)| {
            server.connections.get(pid).map(|k| k.entity_id) == Some(c.state.id)
        }) {
            let e = ent.1;
            println!(
                "  {addr} (игрок {pid}): позиция = ({:.1}, {:.1}, {:.1})",
                e.state.pos.x, e.state.pos.y, e.state.pos.z
            );
        }
    }
    let _ = InputFrame::neutral(0);
    let _ = Snapshot { tick: 0, ack_input_seq: 0, baseline: true, entities: vec![] };
}
