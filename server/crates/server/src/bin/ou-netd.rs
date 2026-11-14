//! Open Universe — авторитетный сервер по реальному UDP.
//!
//! Запуск: `cargo run -p ou-server --bin ou-netd -- [адрес] [секунды] [каталог]`
//! По умолчанию: `127.0.0.1:7777`, 4 секунды, каталог `./world`.
//!
//! Мир персистентен: при старте загружается последний снимок, периодически
//! сохраняется и сохраняется на выходе.

use ou_net::{new_session_token, UdpTransport};
use ou_persist::{save_world, FileStore};
use ou_protocol::{BuildReason, InputFrame, Message, NetId, Snapshot, Vec3, TICK_DT};
use ou_server::{Server, INTEREST_RADIUS};
use ou_sim::Dent;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Как часто сохранять мир (в тиках). 30 Гц × 300 = раз в 10 секунд.
const AUTOSAVE_TICKS: u64 = 300;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:7777".to_string());
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let world_dir = args.get(3).cloned().unwrap_or_else(|| "world".to_string());

    let token = new_session_token(0x5EED);
    let mut net = UdpTransport::bind(&addr).expect("не удалось занять UDP-порт");
    println!("ou-netd: слушаю {}", net.local_addr().unwrap());
    println!("токен сессии: {token:#018x} | тик 30 Гц | радиус интереса {INTEREST_RADIUS}");

    // --- персистентность: загружаем мир ---
    let mut store = FileStore::open(&world_dir).expect("не удалось открыть каталог мира");
    let mut server = Server::new();
    server.build_system();
    match ou_persist::load_world(&store) {
        Ok(w) if !w.entities.is_empty() => {
            println!(
                "мир загружен из «{world_dir}»: {} сущностей, тик {}",
                w.entities.len(),
                w.tick
            );
            server.adopt_world(w);
        }
        Ok(_) => println!("мир пуст — начинаем новый (каталог «{world_dir}»)"),
        Err(e) => println!("снимок мира не прочитан ({e}) — начинаем новый"),
    }

    // Мишени-дроны у точки появления: цель для проверки боя. Висят невысоко
    // над поверхностью, в стороне от игрока, чтобы в них можно было стрелять.
    {
        let base = ou_sim::home_spawn(2.0);
        let up = {
            let l = (base.x * base.x + base.y * base.y + base.z * base.z).sqrt();
            if l > 1e-6 { Vec3::new(base.x / l, base.y / l, base.z / l) } else { Vec3::new(0.0, 1.0, 0.0) }
        };
        // Касательный «вперёд» (мировой +X, спроецированный на плоскость).
        let mut fwd = Vec3::new(1.0, 0.0, 0.0);
        let d = fwd.dot(up);
        fwd = Vec3::new(fwd.x - up.x * d, fwd.y - up.y * d, fwd.z - up.z * d);
        let fl = (fwd.x * fwd.x + fwd.y * fwd.y + fwd.z * fwd.z).sqrt();
        if fl > 1e-6 { fwd = Vec3::new(fwd.x / fl, fwd.y / fl, fwd.z / fl); }
        for i in 0..3 {
            let dist = 40.0 + i as f64 * 30.0;   // 40/70/100 м впереди
            let side = if i % 2 == 0 { 1.0 } else { -1.0 } * 20.0;
            let p = Vec3::new(
                base.x + fwd.x * dist + up.x * 15.0,
                base.y + fwd.y * dist + up.y * 15.0,
                base.z + fwd.z * dist + up.z * 15.0,
            );
            // side — по касательному «вправо» (упрощённо, через up×fwd).
            let right = Vec3::new(
                up.y * fwd.z - up.z * fwd.y,
                up.z * fwd.x - up.x * fwd.z,
                up.x * fwd.y - up.y * fwd.x,
            );
            let p = Vec3::new(p.x + right.x * side, p.y + right.y * side, p.z + right.z * side);
            let id = server.spawn_target(p);
            println!("дрон-мишень {id} в ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z);
        }
    }

    // Орбитальные цели у луны: космический бой вдали от поверхности. Луна —
    // тело id 2 в (+560000,0,0), радиус 60000; цели висят на орбите вокруг неё.
    {
        let moon_center = Vec3::new(560_000.0, 0.0, 0.0);
        let spawns = server.spawn_orbit_targets(moon_center, 60_000.0, 3);
        for (i, p) in spawns.iter().enumerate() {
            println!("орбитальная цель {i} у луны в ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z);
        }
    }
    println!("работаю {seconds} с...\n");

    let mut clients: BTreeMap<SocketAddr, NetId> = BTreeMap::new();
    // Ответы на команды строительства (кому и что ответить).
    let mut build_results: Vec<(SocketAddr, BuildReason)> = Vec::new();
    // Продолжаем нумерацию игроков с учётом загруженного мира.
    let mut next_player: NetId = server.next_entity_id();

    let tick_dt = Duration::from_secs_f64(TICK_DT);
    let start = Instant::now();
    let mut ticks: u64 = 0;
    let mut bytes_out: u64 = 0;
    let mut snapshots_out: u64 = 0;
    let mut saves: u64 = 0;
    let mut saved_bytes: u64 = 0;
    /// Воронки, применённые на этом тике: (слот, воронка) — рассылаем всем.
    let mut pending_digs: Vec<(u32, Dent)> = Vec::new();
    let mut pending_yields: Vec<(std::net::SocketAddr, Message)> = Vec::new();

    while start.elapsed().as_secs_f64() < seconds {
        let loop_start = Instant::now();

        // --- приём ---
        loop {
            match net.recv_message(token) {
                Ok(Some((from, msg))) => match msg {
                    Message::ClientHello { name, .. } => {
                        if !clients.contains_key(&from) {
                            // Игрок появляется на домашней планете (на суше),
                            // с привязкой к рельефу: гравитация и посадка.
                            let entity = server.connect_on_planet(next_player);
                            clients.insert(from, next_player);
                            let p = server
                                .world
                                .entities
                                .get(&entity)
                                .map(|e| e.state.pos)
                                .unwrap_or(Vec3::ZERO);
                            println!(
                                "игрок {next_player} «{name}» вошёл с {from} (сущность {entity}, спавн {:.0},{:.0},{:.0})",
                                p.x, p.y, p.z
                            );
                            // Сообщаем клиенту, какой сущностью он управляет.
                            net.send_to(token, from, &Message::ServerWelcome { entity }).ok();
                            // И отдаём описание системы (солнце и планеты) для отрисовки.
                            let (sid, sname, bodies) = server.system_description();
                            net.send_to(
                                token,
                                from,
                                &Message::ServerSystem { id: sid, name: sname, bodies },
                            )
                            .ok();
                            // И каталог рецептов производства (окно «Производство»).
                            net.send_to(
                                token,
                                from,
                                &Message::ServerRecipes { recipes: server.recipe_list() },
                            )
                            .ok();
                            // И территории системы (владение, статус, налог).
                            net.send_to(
                                token,
                                from,
                                &Message::ServerTerritory { territories: server.territory_list() },
                            )
                            .ok();
                            // И библиотека скриптов (окно «Скрипты»).
                            net.send_to(
                                token,
                                from,
                                &Message::ServerScripts { templates: server.script_templates() },
                            )
                            .ok();
                            // И библиотека шаблонов постройки (редактор).
                            net.send_to(
                                token,
                                from,
                                &Message::ServerBlueprints { templates: server.blueprint_templates() },
                            )
                            .ok();
                            // И витрина мастерской (шаблоны игроков).
                            net.send_to(token, from, &server.workshop_state(next_player)).ok();
                            // И состояние организации игрока.
                            net.send_to(token, from, &server.org_state(next_player)).ok();
                            next_player += 1;
                        }
                    }
                    Message::ClientInput { inputs } => {
                        if let Some(pid) = clients.get(&from) {
                            server.receive_input(*pid, &inputs);
                        }
                    }
                    Message::ClientCharacter { entity, pos, yaw, moving, active } => {
                        if clients.contains_key(&from) {
                            server.receive_character(entity, pos, yaw, moving, active);
                        }
                    }
                    Message::ClientBuild(cmd) => {
                        if let Some(pid) = clients.get(&from) {
                            let reason = server.apply_build_as(*pid, &cmd);
                            build_results.push((from, reason));
                        }
                    }
                    Message::ClientBuildAck { entity, revision } => {
                        if let Some(pid) = clients.get(&from) {
                            server.ack_build(*pid, entity, revision);
                        }
                    }
                    Message::ClientDig { slot, dir, cos_radius, depth } => {
                        if let Some(pid) = clients.get(&from) {
                            // Авторитетно применяем воронку и рассылаем всем;
                            // игроку начисляется руда (источник сырья).
                            let (dent, (item, qty), hole_depth) = server.dig_as(*pid, slot, dir, cos_radius, depth);
                            pending_digs.push((slot, dent));
                            // Автору сообщаем, что и с какой глубины он добыл.
                            let layer = ou_server::Economy::ore_layer(hole_depth);
                            let yield_msg = Message::ServerDigYield {
                                item,
                                qty,
                                layer,
                                hole_depth,
                            };
                            pending_yields.push((from, yield_msg));
                        }
                    }
                    Message::ClientMarket { side, item, price, qty } => {
                        if let Some(pid) = clients.get(&from) {
                            match server.place_order(*pid, side, item, price, qty) {
                                Ok(filled) => println!(
                                    "рынок: игрок {pid} выставил ордер (сторона {side}, предмет {item}, цена {price}, {qty} шт), исполнено {filled}"
                                ),
                                Err(e) => println!("рынок: ордер игрока {pid} отклонён: {e}"),
                            }
                        }
                    }
                    Message::ClientGrantBuild { entity, player } => {
                        if let Some(pid) = clients.get(&from) {
                            server.grant_build(*pid, entity, player);
                        }
                    }
                    Message::ClientPing { nonce } => {
                        // Эхо пинга: клиент по нему считает реальный RTT.
                        net.send_to(token, from, &Message::ServerPong { nonce }).ok();
                    }
                    Message::ClientCraft { recipe } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.craft_as(*pid, recipe) {
                                Ok((item, qty)) => Message::ServerCraftResult {
                                    ok: true,
                                    item,
                                    qty,
                                    reason: String::new(),
                                },
                                Err(e) => Message::ServerCraftResult {
                                    ok: false,
                                    item: 0,
                                    qty: 0,
                                    reason: e,
                                },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientCapture { id } => {
                        if let Some(pid) = clients.get(&from) {
                            match server.capture_as(*pid, id) {
                                Ok(tid) => println!("территория: игрок {pid} захватил зону {tid}"),
                                Err(e) => println!("территория: захват игроком {pid} отклонён: {e}"),
                            }
                        }
                    }
                    Message::ClientSetScript { template } => {
                        if let Some(pid) = clients.get(&from) {
                            match server.set_script_template(*pid, template) {
                                Ok(0) => println!("скрипт: игрок {pid} снял скрипт"),
                                Ok(tid) => println!("скрипт: игрок {pid} привязал шаблон {tid}"),
                                Err(e) => println!("скрипт: привязка игроком {pid} отклонена: {e}"),
                            }
                        }
                    }
                    Message::ClientPlaceBlueprint { id, origin, entity } => {
                        if let Some(pid) = clients.get(&from) {
                            match server.place_blueprint_as(*pid, entity, id, origin) {
                                Ok(n) => println!("шаблон: игрок {pid} поставил {n} блоков (шаблон {id})"),
                                Err(e) => println!("шаблон: игрок {pid}: {e}"),
                            }
                        }
                    }
                    Message::ClientPublishBlueprint { name, price } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.publish_blueprint(*pid, name, price) {
                                Ok(id) => Message::ServerWorkshopResult {
                                    ok: true,
                                    message: format!("шаблон опубликован (id {id})"),
                                },
                                Err(e) => Message::ServerWorkshopResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientBuyBlueprint { id } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.buy_blueprint(*pid, id) {
                                Ok(()) => Message::ServerWorkshopResult {
                                    ok: true,
                                    message: "шаблон куплен".to_string(),
                                },
                                Err(e) => Message::ServerWorkshopResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientPlaceOwnBlueprint { id, origin, entity } => {
                        if let Some(pid) = clients.get(&from) {
                            match server.place_own_blueprint(*pid, entity, id, origin) {
                                Ok(n) => println!("мастерская: игрок {pid} поставил свой шаблон {id} ({n} блоков)"),
                                Err(e) => println!("мастерская: игрок {pid}: {e}"),
                            }
                        }
                    }
                    Message::ClientCreateOrg { name, tag } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.create_org(*pid, name, tag) {
                                Ok(id) => Message::ServerOrgResult {
                                    ok: true,
                                    message: format!("организация создана (id {id})"),
                                },
                                Err(e) => Message::ServerOrgResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientJoinOrg { id } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.join_org(*pid, id) {
                                Ok(()) => Message::ServerOrgResult { ok: true, message: "вы вступили".to_string() },
                                Err(e) => Message::ServerOrgResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientLeaveOrg => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.leave_org(*pid) {
                                Ok(()) => Message::ServerOrgResult { ok: true, message: "вы вышли".to_string() },
                                Err(e) => Message::ServerOrgResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientSetOrgRank { player, rank } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.set_org_rank(*pid, player, rank) {
                                Ok(()) => Message::ServerOrgResult { ok: true, message: "ранг изменён".to_string() },
                                Err(e) => Message::ServerOrgResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    Message::ClientDeclareDiplomacy { org, stance } => {
                        if let Some(pid) = clients.get(&from) {
                            let msg = match server.declare_diplomacy(*pid, org, stance) {
                                Ok(()) => Message::ServerOrgResult { ok: true, message: "дипломатия объявлена".to_string() },
                                Err(e) => Message::ServerOrgResult { ok: false, message: e },
                            };
                            net.send_to(token, from, &msg).ok();
                        }
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // --- ответы на команды строительства ---
        for (addr, reason) in build_results.drain(..) {
            let accepted = reason == BuildReason::Ok;
            net.send_to(token, addr, &Message::ServerBuildResult { accepted, reason }).ok();
        }

        // --- рассылка применённых воронок ВСЕМ (включая автора) ---
        for (slot, d) in pending_digs.drain(..) {
            let msg = Message::ServerTerrainDig {
                slot,
                dir: Vec3::new(d.x, d.y, d.z),
                cos_radius: d.cos_r,
                depth: d.depth,
            };
            for addr in clients.keys() {
                net.send_to(token, *addr, &msg).ok();
            }
        }

        // --- что добыл игрок (только автору удара) ---
        for (addr, msg) in pending_yields.drain(..) {
            net.send_to(token, addr, &msg).ok();
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

        // --- автосохранение мира ---
        if ticks % AUTOSAVE_TICKS == 0 {
            match save_world(&mut store, &server.world) {
                Ok(n) => {
                    saves += 1;
                    saved_bytes += n;
                    println!("[автосейв] тик {ticks}: сохранено {n} байт");
                }
                Err(e) => println!("[автосейв] ошибка: {e}"),
            }
        }

        // --- выдержка частоты ---
        let elapsed = loop_start.elapsed();
        if elapsed < tick_dt {
            sleep(tick_dt - elapsed);
        }
    }

    // --- финальное сохранение при выходе ---
    match save_world(&mut store, &server.world) {
        Ok(n) => {
            saves += 1;
            saved_bytes += n;
            println!("\n[выход] мир сохранён: {n} байт, тик {}", server.world.tick);
        }
        Err(e) => println!("\n[выход] не удалось сохранить мир: {e}"),
    }

    println!("\n--- итоги ---");
    println!("тиков симуляции: {ticks}");
    println!("игроков обслужено: {}", clients.len());
    println!("снапшотов отправлено: {snapshots_out}");
    println!("байт отправлено: {bytes_out}");
    println!("сохранений мира: {saves} ({saved_bytes} байт всего)");
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
