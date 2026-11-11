//! # ou-client
//!
//! Headless‑клиент Open Universe для проверки сетевой модели: клиентское
//! предсказание и реконсиляция. Здесь нет рендера — только логика, которая
//! потом переедет в UE5.

use ou_protocol::{InputFrame, NetId};
use ou_sim::World;

/// Клиентская копия мира: предсказывает движение, чтобы играть без задержки.
pub struct PredictedClient {
    pub world: World,
    /// Кадры, отправленные, но ещё не подтверждённые сервером.
    pub pending: Vec<InputFrame>,
    pub last_acked_seq: u32,
}

impl PredictedClient {
    pub fn new(entity: NetId) -> Self {
        let mut world = World::new();
        world.spawn(entity, ou_protocol::Vec3::ZERO);
        PredictedClient { world, pending: Vec::new(), last_acked_seq: 0 }
    }

    /// Локальный ввод: сразу применяем (предсказание) и ставим в очередь на отправку.
    pub fn local_input(&mut self, input: InputFrame, entity: NetId) {
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert(entity, input);
        self.world.step(&inputs);
        self.pending.push(input);
    }

    /// Пришло подтверждение сервера: выкидываем подтверждённые кадры.
    pub fn ack(&mut self, last_input_seq: u32) {
        self.last_acked_seq = last_input_seq;
        self.pending.retain(|i| i.seq > last_input_seq);
    }

    /// Реконсиляция: применяем авторитетное состояние сервера и заново
    /// проигрываем только неподтверждённые кадры.
    ///
    /// Порядок важен: сначала отбрасываем кадры, которые сервер уже применил
    /// (`ack_seq`), затем ставим серверное состояние и переигрываем остаток.
    /// Иначе подтверждённые кадры применятся повторно.
    pub fn reconcile(
        &mut self,
        authoritative: &[ou_protocol::EntityState],
        entity: NetId,
        ack_seq: u32,
    ) {
        // 1. Убрать подтверждённые кадры.
        self.ack(ack_seq);

        // 2. Встать на авторитетную точку.
        if let Some(server_state) = authoritative.iter().find(|e| e.id == entity) {
            if let Some(ent) = self.world.entities.get_mut(&entity) {
                ent.state = *server_state;
            }
        }

        // 3. Переиграть только то, что сервер ещё не подтвердил.
        let replay: Vec<InputFrame> = self.pending.clone();
        for input in replay {
            let mut inputs = std::collections::BTreeMap::new();
            inputs.insert(entity, input);
            self.world.step(&inputs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ou_sim::World;
    use std::collections::BTreeMap;

    #[test]
    fn prediction_matches_authoritative_when_no_loss() {
        // Сервер и клиент должны сойтись при одинаковых входных данных.
        let mut server = World::new();
        server.spawn(1, ou_protocol::Vec3::ZERO);
        let mut client = PredictedClient::new(1);

        for seq in 0..120u32 {
            let input = InputFrame {
                seq,
                throttle: 1.0,
                yaw: ou_sim::det_sin(seq as f64) as f32 * 0.3,
                pitch: 0.0,
                fire: false,
            };
            let mut m = BTreeMap::new();
            m.insert(1, input);
            server.step(&m);
            client.local_input(input, 1);
            client.ack(seq);
        }
        // Идеальная сеть → расхождение нулевое.
        let sc = server.checksum();
        let cc = client.world.checksum();
        assert_eq!(sc, cc, "предсказание должно совпасть с сервером");
    }

    #[test]
    fn reconcile_resolves_divergence_after_loss() {
        let mut server = World::new();
        server.spawn(1, ou_protocol::Vec3::ZERO);
        let mut client = PredictedClient::new(1);

        // 1. Первые 10 тиков — согласованно.
        for seq in 0..10u32 {
            let input = InputFrame { seq, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false };
            let mut m = BTreeMap::new();
            m.insert(1, input);
            server.step(&m);
            client.local_input(input, 1);
            client.ack(seq);
        }
        assert_eq!(server.checksum(), client.world.checksum());

        // 2. Клиент предсказывает 5 кадров вперёд, сервер их ещё не получил.
        let mut pending = Vec::new();
        for seq in 10..15u32 {
            let input = InputFrame { seq, throttle: 1.0, yaw: 0.5, pitch: 0.0, fire: false };
            client.local_input(input, 1);
            pending.push(input);
        }
        assert_ne!(server.checksum(), client.world.checksum(), "клиент должен быть впереди");

        // 3. Сервер присылает авторитетный снимок (тик 10, ack = 9).
        //    Пока он не получил кадры 10..14, расхождение законно: клиент — предсказание.
        client.reconcile(&[server.snapshot_map()[&1]], 1, 9);
        // Клиент = сервер(tick 10) + переигранные 5 кадров = предсказание на тик 15.

        // 4. Сервер наконец получает те же кадры и догоняет.
        for input in &pending {
            let mut m = BTreeMap::new();
            m.insert(1, *input);
            server.step(&m);
        }

        // 5. Теперь сервер и предсказание клиента обязаны совпасть.
        assert_eq!(
            server.checksum(),
            client.world.checksum(),
            "после доставки вводов предсказание должно сойтись с сервером"
        );
    }
}
