//! Open Universe — сетевой рендер-клиент.
//!
//! Подключается к серверу (`ou-netd`) по UDP, принимает дельта-снапшоты,
//! собирает из них состояние мира и **рисует его в реальном времени** своим
//! трассировщиком лучей. Периодически сохраняет кадры в PNG.
//!
//! Запуск:
//!   cargo run -p ou-render --release --bin ou-netrender -- [адрес] [секунды] [кадров]
//! По умолчанию: 127.0.0.1:7777, 6 секунд, 4 кадра.
//!
//! ## Что здесь важно
//! Это полный цикл движка MMO: сервер → протокол → сеть → клиентская сборка
//! мира (baseline + дельты) → рендер. Код сборки мира (`apply_snapshot`) — тот
//! же, что понадобится клиенту на UE5.

use ou_net::{new_session_token, UdpTransport};
use ou_protocol::{EntityState, InputFrame, Message, NetId, Snapshot, Vec3, TICK_DT};
use ou_render::{contact_sheet, render, scene_from_states, png};
use ou_sim::det_sin;
use std::collections::BTreeMap;
use std::net::UdpSocket;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Клиентский мир: что мы знаем о сущностях, собранное из baseline и дельт.
#[derive(Default)]
struct ClientWorld {
    entities: BTreeMap<NetId, EntityState>,
    /// Сколько дельт применили (для статистики).
    deltas: u64,
    /// Сколько baseline получили.
    baselines: u64,
}

impl ClientWorld {
    /// Применить снимок: baseline заменяет мир, дельта — только изменения.
    fn apply(&mut self, snap: &Snapshot) {
        if snap.baseline {
            self.entities.clear();
            self.baselines += 1;
        } else {
            self.deltas += 1;
        }
        for e in &snap.entities {
            self.entities.insert(e.id, *e);
        }
    }

    fn states(&self) -> Vec<EntityState> {
        self.entities.values().copied().collect()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:7777".to_string());
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let frames: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);

    let token = new_session_token(0x5EED);
    let sock = UdpSocket::bind("127.0.0.1:0").expect("локальный порт");
    let mut net = UdpTransport::from_socket(sock).expect("транспорт");
    net.set_nonblocking(true).ok();
    let server: std::net::SocketAddr = addr.parse().expect("неверный адрес");

    println!("ou-netrender -> {server} (токен {token:#018x})");
    println!("работаю {seconds} с, сохраняю {frames} кадров...\n");

    // Приветствие.
    net.send_to(token, server, &Message::ClientHello { protocol: 1, name: "renderer".into() })
        .expect("hello");
    net.flush().expect("flush");

    let mut world = ClientWorld::default();
    let mut my_entity: Option<NetId> = None;
    let mut seq: u32 = 0;
    let mut captured: Vec<(u32, u32, Vec<u8>)> = Vec::new();

    let start = Instant::now();
    let frame_dt = Duration::from_secs_f64(TICK_DT);
    // Кадры берём равномерно, заканчивая чуть раньше общего времени, чтобы
    // последний кадр точно успел сохраниться.
    let capture_every = (seconds * 0.85 / frames as f64).max(0.2);
    let mut next_capture = capture_every;
    let w = 640u32;
    let h = 360u32;

    while start.elapsed().as_secs_f64() < seconds {
        let loop_start = Instant::now();

        // --- приём снимков ---
        loop {
            match net.recv_message(token) {
                Ok(Some((_from, msg))) => match msg {
                    Message::ServerWelcome { entity } => {
                        my_entity = Some(entity);
                    }
                    Message::ServerSnapshot(snap) => {
                        if my_entity.is_none() {
                            if let Some(e) = snap.entities.first() {
                                my_entity = Some(e.id);
                            }
                        }
                        world.apply(&snap);
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // --- небольшой ввод, чтобы корабль маневрировал ---
        seq += 1;
        let input = InputFrame {
            seq,
            throttle: 1.0,
            yaw: (det_sin(seq as f64 * 0.04) * 0.35) as f32,
            pitch: (det_sin(seq as f64 * 0.02 + 1.0) * 0.08) as f32,
            fire: false,
        };
        net.send_to(token, server, &Message::ClientInput { inputs: vec![input] }).ok();
        net.flush().ok();

        // --- рендер кадра мира, если пришло время и есть что рисовать ---
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed >= next_capture && captured.len() < frames && !world.entities.is_empty() {
            let focus = my_entity.unwrap_or(1);
            let states = world.states();
            let scene = scene_from_states(&states, focus);
            let buf = render(&scene, w, h);
            println!(
                "  кадр {:>2} на {:.1} c: сущностей {}, baseline {}, дельт {}",
                captured.len() + 1,
                elapsed,
                states.len(),
                world.baselines,
                world.deltas
            );
            captured.push((w, h, buf));
            next_capture += capture_every;
        }

        let spent = loop_start.elapsed();
        if spent < frame_dt {
            sleep(frame_dt - spent);
        }
    }

    // --- сохранение ---
    println!("\n--- итоги сетевого рендера ---");
    println!("получено baseline: {}", world.baselines);
    println!("получено дельт: {}", world.deltas);
    println!("сущностей в мире: {}", world.entities.len());

    if captured.is_empty() {
        println!("кадров не сохранено (сервер не прислал состояние)");
        return;
    }
    if captured.len() == 1 {
        let (fw, fh, data) = &captured[0];
        png::write_png("render-net.png", *fw, *fh, data).ok();
        println!("сохранён 1 кадр -> render-net.png");
    } else {
        // Столбцов ровно столько, сколько кадров — без пустых ячеек.
        let cols = captured.len().min(4);
        if let Some((sw, sh, data)) = contact_sheet(&captured, cols) {
            png::write_png("render-net.png", sw, sh, &data).ok();
            println!("сохранено {} кадров -> render-net.png ({}×{})", captured.len(), sw, sh);
        }
    }
    let _ = Vec3::ZERO;
}
