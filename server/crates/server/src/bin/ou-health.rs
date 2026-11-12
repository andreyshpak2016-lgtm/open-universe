//! Open Universe — самопроверка «сервер + клиент».
//!
//! Что делает (за один прогон, без внешних сервисов):
//!   1. поднимает авторитетный сервер `ou-netd` на свободном порту;
//!   2. подключается к нему реальным UDP-клиентом (`ou-netclient`-подобным);
//!   3. ждёт `ServerWelcome` и первые снимки (значит, симуляция идёт);
//!   4. проверяет, что позиция корабля — у поверхности домашней планеты;
//!   5. пишет `status.json` для сайта и лаунчера.
//!
//! Запуск:
//!   `cargo run -p ou-server --bin ou-health -- [out.json]`
//!
//! Расширенный статус (для сайта/лаунчера) кладётся рядом. Если что-то не
//! поднялось — в статусе будет `"ok": false` и текст ошибки, а код возврата 1.

use ou_net::{new_session_token, UdpTransport};
use ou_protocol::{InputFrame, Message, NetId, TICK_DT};
use ou_sim::{home_planet, normalize, Vec3};
use ou_server::Server;
use std::net::{SocketAddr, UdpSocket};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Результат самопроверки — то, что попадает в `status.json`.
struct Health {
    ok: bool,
    server: bool,
    client: bool,
    simulation: bool,
    spawn_on_planet: bool,
    spawn_height_above_surface: f64,
    tick: u64,
    entity_id: NetId,
    latency_ms: f64,
    notes: String,
}

#[derive(Default)]
struct Json {
    // Мини-сериализатор: держим файл простым и читаемым, без зависимостей.
    fields: Vec<(String, String)>,
}

impl Json {
    fn str(&mut self, k: &str, v: &str) {
        let esc = v.replace('\\', "\\\\").replace('"', "\\\"");
        self.fields.push((k.to_string(), format!("\"{esc}\"")));
    }
    fn num(&mut self, k: &str, v: f64) {
        self.fields.push((k.to_string(), format!("{v}")));
    }
    fn int(&mut self, k: &str, v: i64) {
        self.fields.push((k.to_string(), format!("{v}")));
    }
    fn bool(&mut self, k: &str, v: bool) {
        self.fields.push((k.to_string(), if v { "true".into() } else { "false".into() }));
    }
    fn to_string(&self) -> String {
        let inner: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("    \"{k}\": {v}"))
            .collect();
        format!("{{\n{}\n}}\n", inner.join(",\n"))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "status.json".to_string());

    println!("== Open Universe: самопроверка сервер+клиент ==");

    let h = run_check();

    // --- status.json для сайта и лаунчера ---
    let mut j = Json::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    j.int("generated", now as i64);
    j.bool("ok", h.ok);
    j.bool("server", h.server);
    j.bool("client", h.client);
    j.bool("simulation", h.simulation);
    j.bool("spawnOnPlanet", h.spawn_on_planet);
    j.num("spawnHeightAboveSurface", h.spawn_height_above_surface);
    j.int("tick", h.tick as i64);
    j.int("entityId", h.entity_id as i64);
    j.num("latencyMs", h.latency_ms);
    j.str("planet", "home");
    j.str("notes", &h.notes);
    if let Err(e) = std::fs::write(&out, j.to_string()) {
        eprintln!("не удалось записать {out}: {e}");
    } else {
        println!("статус записан: {out}");
    }

    println!("\n--- итог ---");
    println!("сервер: {} | клиент: {} | симуляция: {} | спавн на планете: {}",
        yes(h.server), yes(h.client), yes(h.simulation), yes(h.spawn_on_planet));
    println!("тик: {} | сущность: {} | над поверхностью: {:.1} | задержка: {:.0} мс",
        h.tick, h.entity_id, h.spawn_height_above_surface, h.latency_ms);
    if !h.notes.is_empty() {
        println!("примечание: {}", h.notes);
    }

    if !h.ok {
        std::process::exit(1);
    }
}

fn yes(b: bool) -> &'static str {
    if b { "да" } else { "нет" }
}

/// Прогнать полный цикл «сервер + клиент» в памяти (без внешних процессов).
///
/// Авторитетный сервер гоняем как `Server` напрямую, а клиент — реальным UDP:
/// для него поднимаем настоящий сокет сервера на свободном порту. Так проверка
/// ловит и код симуляции, и сетевой стек.
fn run_check() -> Health {
    let mut h = Health {
        ok: false,
        server: false,
        client: false,
        simulation: false,
        spawn_on_planet: false,
        spawn_height_above_surface: 0.0,
        tick: 0,
        entity_id: 0,
        latency_ms: 0.0,
        notes: String::new(),
    };

    // --- 1. Реальный UDP-сервер на свободном порту (в отдельном потоке) ---
    let bind = match UdpSocket::bind("127.0.0.1:0") {
        Ok(s) => s,
        Err(e) => {
            h.notes = format!("не удалось занять UDP-порт: {e}");
            return h;
        }
    };
    let addr: SocketAddr = match bind.local_addr() {
        Ok(a) => a,
        Err(e) => {
            h.notes = format!("не удалось узнать адрес: {e}");
            return h;
        }
    };
    drop(bind); // освобождаем порт — сервер займёт его сам

    let token = new_session_token(0x5EED);
    let server_addr = addr;
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = stop.clone();
    let server_thread = std::thread::spawn(move || {
        run_udp_server(server_addr, token, server_stop);
    });

    // Даём серверу подняться.
    sleep(Duration::from_millis(200));

    // --- 2. Клиент подключается по UDP ---
    let client_sock = match UdpSocket::bind("127.0.0.1:0") {
        Ok(s) => s,
        Err(e) => {
            h.notes = format!("клиент не смог занять порт: {e}");
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = server_thread.join();
            return h;
        }
    };
    let mut net = match UdpTransport::from_socket(client_sock) {
        Ok(n) => n,
        Err(e) => {
            h.notes = format!("транспорт клиента: {e}");
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = server_thread.join();
            return h;
        }
    };
    net.set_nonblocking(true).ok();

    let t0 = Instant::now();
    net.send_to(token, server_addr, &Message::ClientHello { protocol: 1, name: "health".into() })
        .ok();
    net.flush().ok();
    h.client = true;

    // --- 3. Ждём welcome и снимки ---
    let mut me: Option<NetId> = None;
    let mut my_state = None;
    let mut snapshots = 0u32;
    let mut seq: u32 = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    let frame = Duration::from_secs_f64(TICK_DT);

    while Instant::now() < deadline {
        let loop_start = Instant::now();
        loop {
            match net.recv_message(token) {
                Ok(Some((_from, msg))) => match msg {
                    Message::ServerWelcome { entity } => {
                        me = Some(entity);
                        h.entity_id = entity;
                        h.latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
                    }
                    Message::ServerSnapshot(snap) => {
                        snapshots += 1;
                        h.tick = snap.tick;
                        let id = *me.get_or_insert_with(|| {
                            snap.entities.first().map(|e| e.id).unwrap_or(1)
                        });
                        if let Some(e) = snap.entities.iter().find(|e| e.id == id) {
                            my_state = Some(e.pos);
                        }
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // Отправляем ввод: сервер должен двигать корабль.
        if me.is_some() {
            seq += 1;
            let input = InputFrame { seq, throttle: 0.0, yaw: 0.0, pitch: 0.0, fire: false };
            net.send_to(token, server_addr, &Message::ClientInput { inputs: vec![input] }).ok();
            net.flush().ok();
        }

        // Достаточно данных: welcome + несколько снимков.
        if me.is_some() && snapshots >= 5 {
            break;
        }

        let elapsed = loop_start.elapsed();
        if elapsed < frame {
            sleep(frame - elapsed);
        }
    }

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = server_thread.join();

    h.server = h.entity_id != 0;
    h.simulation = snapshots >= 5 && h.tick > 0;

    // --- 4. Проверяем, что спавн — у поверхности домашней планеты ---
    if let Some(pos) = my_state {
        let t = home_planet();
        let dir = normalize(pos);
        let surface = t.surface_radius(dir);
        let dist = pos.length();
        h.spawn_height_above_surface = dist - surface;
        // Спавн над поверхностью в разумных пределах (не в космосе и не под землёй).
        h.spawn_on_planet = dist > surface - 5.0 && dist < surface + 400.0;
    }

    h.ok = h.server && h.client && h.simulation && h.spawn_on_planet;
    if !h.ok && h.notes.is_empty() {
        h.notes = if !h.server {
            "сервер не ответил (welcome не пришёл)".to_string()
        } else if !h.simulation {
            format!("снимков получено: {snapshots}")
        } else if !h.spawn_on_planet {
            format!(
                "спавн не у поверхности планеты: {:.1} над поверхностью",
                h.spawn_height_above_surface
            )
        } else {
            String::new()
        };
    }
    h
}

/// Настоящий UDP-сервер: тот же цикл, что в `ou-netd`, но для самопроверки.
fn run_udp_server(addr: SocketAddr, token: u64, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::collections::BTreeMap;
    let mut net = match UdpTransport::bind(&addr.to_string()) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[health-сервер] порт занят: {e}");
            return;
        }
    };
    let mut server = Server::new();
    let mut clients: BTreeMap<SocketAddr, NetId> = BTreeMap::new();
    let mut next_player: NetId = server.next_entity_id();
    let tick_dt = Duration::from_secs_f64(TICK_DT);

    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        let loop_start = Instant::now();

        loop {
            match net.recv_message(token) {
                Ok(Some((from, msg))) => match msg {
                    Message::ClientHello { .. } => {
                        if !clients.contains_key(&from) {
                            let entity = server.connect_on_planet(next_player);
                            clients.insert(from, next_player);
                            net.send_to(token, from, &Message::ServerWelcome { entity }).ok();
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

        let plan = server.tick();
        for (player_id, msgs) in &plan.messages {
            if let Some((addr, _)) = clients.iter().find(|(_, pid)| *pid == player_id) {
                for m in msgs {
                    net.send_to(token, *addr, m).ok();
                }
            }
        }
        net.flush().ok();

        let elapsed = loop_start.elapsed();
        if elapsed < tick_dt {
            sleep(tick_dt - elapsed);
        }
    }
    let _ = Vec3::ZERO;
}
