//! Демонстрация клиентского предсказания и реконсиляции.

use ou_client::PredictedClient;
use ou_protocol::{InputFrame, Message, Snapshot, Vec3};
use ou_sim::World;
use std::collections::BTreeMap;

fn main() {
    println!("Open Universe — headless-клиент: предсказание и реконсиляция\n");

    let mut server = World::new();
    server.spawn(1, Vec3::ZERO);
    let mut client = PredictedClient::new(1);

    // --- Фаза 1: идеальная сеть ---
    for seq in 0..60u32 {
        let input = InputFrame { seq, throttle: 1.0, yaw: 0.1, pitch: 0.0, fire: false };
        let mut m = BTreeMap::new();
        m.insert(1, input);
        server.step(&m);
        client.local_input(input, 1);
        client.ack(seq);
        // Пример сериализации снапшота (проверка протокола).
        if seq == 0 {
            let snap = Snapshot {
                tick: server.tick,
                ack_input_seq: seq,
                baseline: true,
                entities: vec![server.snapshot_map()[&1]],
            };
            let bytes = ou_protocol::encode(&Message::ServerSnapshot(snap));
            println!("Размер снапшота одной сущности по проводу: {} байт", bytes.len());
        }
    }
    let matched = server.checksum() == client.world.checksum();
    println!("Идеальная сеть: предсказание совпало с сервером -> {matched}");

    // --- Фаза 2: потеря пакетов ---
    // Клиент предсказывает 5 кадров вперёд; до сервера они ещё не дошли.
    let mut pending = Vec::new();
    for seq in 60..65u32 {
        let input = InputFrame { seq, throttle: 1.0, yaw: 1.0, pitch: 0.0, fire: false };
        client.local_input(input, 1);
        pending.push(input);
    }
    println!(
        "После предсказания вперёд клиент опережает сервер -> {}",
        server.checksum() != client.world.checksum()
    );

    // --- Фаза 3: реконсиляция ---
    // Сервер присылает снимок (ack = 59): откат на авторитетную точку + переигровка.
    client.reconcile(&[server.snapshot_map()[&1]], 1, 59);
    println!("После реконсиляции переиграно неподтверждённых кадров: {}", client.pending.len());

    // --- Фаза 4: сервер получает те же кадры и догоняет ---
    for input in &pending {
        let mut m = BTreeMap::new();
        m.insert(1, *input);
        server.step(&m);
    }
    let fixed = server.checksum() == client.world.checksum();
    println!("После доставки вводов предсказание совпало с сервером -> {fixed}");
}
