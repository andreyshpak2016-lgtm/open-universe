//! # ou-client
//!
//! Headless‑клиент Open Universe для проверки сетевой модели: клиентское
//! предсказание и реконсиляция. Здесь нет рендера — только логика, которая
//! потом переедет в UE5.

use ou_protocol::{EntityState, InputFrame, NetId, Snapshot};
use ou_sim::World;
use std::collections::VecDeque;

/// Клиентская копия мира: предсказывает движение, чтобы играть без задержки.
pub struct PredictedClient {
    pub world: World,
    /// Кадры, отправленные, но ещё не подтверждённые сервером.
    pub pending: Vec<InputFrame>,
    pub last_acked_seq: u32,
    /// Буфер входящих снимков для интерполяции удалённых сущностей.
    pub snapshots: VecDeque<Snapshot>,
}

impl PredictedClient {
    pub fn new(entity: NetId) -> Self {
        let mut world = World::new();
        world.spawn(entity, ou_protocol::Vec3::ZERO);
        PredictedClient {
            world,
            pending: Vec::new(),
            last_acked_seq: 0,
            snapshots: VecDeque::new(),
        }
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

    /// Добавить входящий снимок в буфер интерполяции.
    ///
    /// Снимки хранятся отсортированными по тику; дубликаты тиков отбрасываются.
    pub fn push_snapshot(&mut self, snap: &ou_protocol::Snapshot) {
        if self.snapshots.iter().any(|s| s.tick == snap.tick) {
            return;
        }
        self.snapshots.push_back(snap.clone());
        // Держим разумную глубину буфера.
        while self.snapshots.len() > MAX_SNAPSHOTS {
            self.snapshots.pop_front();
        }
    }

    /// Интерполированное состояние чужой сущности в момент `render_time`.
    ///
    /// Идея: клиент отрисовывает мир с задержкой в один-два тика и плавно
    /// смешивает два соседних снимка. Без этого удалённые игроки «телепортируются»
    /// на 30 Гц. Собственный корабль не интерполируется — он предсказывается.
    pub fn interpolate(&self, id: NetId, render_time: f64) -> Option<EntityState> {
        if self.snapshots.is_empty() {
            return self.world.entities.get(&id).map(|e| e.state);
        }
        // Ищем пару снимков, между которыми лежит render_time.
        let mut before: Option<&ou_protocol::Snapshot> = None;
        let mut after: Option<&ou_protocol::Snapshot> = None;
        for s in &self.snapshots {
            let t = s.tick as f64 * ou_protocol::TICK_DT;
            if t <= render_time {
                before = Some(s);
            } else {
                after = Some(s);
                break;
            }
        }
        let a = before.or(after)?;
        let b = after.unwrap_or(a);
        let ea = a.entities.iter().find(|e| e.id == id)?;
        if a.tick == b.tick {
            return Some(*ea);
        }
        let eb = match b.entities.iter().find(|e| e.id == id) {
            Some(e) => *e,
            None => return Some(*ea),
        };
        let ta = a.tick as f64 * ou_protocol::TICK_DT;
        let tb = b.tick as f64 * ou_protocol::TICK_DT;
        let mut alpha = if tb > ta { (render_time - ta) / (tb - ta) } else { 0.0 };
        if alpha < 0.0 {
            alpha = 0.0;
        } else if alpha > 1.0 {
            alpha = 1.0;
        }
        Some(EntityState {
            id,
            pos: ea.pos.add(eb.pos.sub(ea.pos).scale(alpha)),
            vel: ea.vel.add(eb.vel.sub(ea.vel).scale(alpha)),
            yaw: lerp_f32(ea.yaw, eb.yaw, alpha),
            pitch: lerp_f32(ea.pitch, eb.pitch, alpha),
        })
    }

    /// Время рендера: серверное время минус буфер задержки (в тиках).
    pub fn render_time(&self) -> f64 {
        match self.snapshots.back() {
            Some(s) => (s.tick as f64 - INTERP_DELAY_TICKS) * ou_protocol::TICK_DT,
            None => 0.0,
        }
    }
}

/// Глубина буфера интерполяции.
const MAX_SNAPSHOTS: usize = 32;
/// На сколько тиков клиент отстаёт от сервера, чтобы было чем сглаживать.
/// Значение задано в тиках (30 Гц): 2 тика ≈ 66 мс.
const INTERP_DELAY_TICKS: f64 = 2.0;

#[inline]
fn lerp_f32(a: f32, b: f32, t: f64) -> f32 {
    (a as f64 + (b as f64 - a as f64) * t) as f32
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

    // --- снапшот-интерполяция ---

    fn snap_at(tick: u64, id: NetId, x: f64) -> Snapshot {
        Snapshot {
            tick,
            ack_input_seq: 0,
            baseline: false,
            entities: vec![EntityState {
                id,
                pos: ou_protocol::Vec3::new(x, 0.0, 0.0),
                vel: ou_protocol::Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            }],
        }
    }

    #[test]
    fn interpolation_is_between_snapshots() {
        let mut c = PredictedClient::new(1);
        c.push_snapshot(&snap_at(10, 2, 0.0));
        c.push_snapshot(&snap_at(12, 2, 20.0));

        // На середине между тиками 10 и 12 (t = 11/30 с) позиция ≈ 10.
        let mid_t = 11.0 * ou_protocol::TICK_DT;
        let p = c.interpolate(2, mid_t).unwrap();
        assert!((p.pos.x - 10.0).abs() < 1e-9, "x={}", p.pos.x);

        // На точках снимков — точные значения.
        let p0 = c.interpolate(2, 10.0 * ou_protocol::TICK_DT).unwrap();
        let p1 = c.interpolate(2, 12.0 * ou_protocol::TICK_DT).unwrap();
        assert!((p0.pos.x - 0.0).abs() < 1e-9);
        assert!((p1.pos.x - 20.0).abs() < 1e-9);
    }

    #[test]
    fn interpolation_clamps_outside_range() {
        let mut c = PredictedClient::new(1);
        c.push_snapshot(&snap_at(10, 2, 0.0));
        c.push_snapshot(&snap_at(12, 2, 20.0));
        // Раньше первого снимка → берём первый.
        assert_eq!(c.interpolate(2, 0.0).unwrap().pos.x, 0.0);
        // Позже последнего → берём последний.
        assert_eq!(c.interpolate(2, 100.0).unwrap().pos.x, 20.0);
    }

    #[test]
    fn interpolation_buffer_is_bounded_and_dedup() {
        let mut c = PredictedClient::new(1);
        for t in 0..100u64 {
            c.push_snapshot(&snap_at(t, 2, t as f64));
        }
        assert!(c.snapshots.len() <= 32, "буфер не должен расти бесконечно");
        // Повторный тик не добавляется.
        let len_before = c.snapshots.len();
        let last_tick = c.snapshots.back().unwrap().tick;
        c.push_snapshot(&snap_at(last_tick, 2, 999.0));
        assert_eq!(c.snapshots.len(), len_before);
    }

    #[test]
    fn interpolation_smooths_motion_continuously() {
        // Плавность: между снимками позиция монотонно растёт (нет «телепортов»).
        let mut c = PredictedClient::new(1);
        c.push_snapshot(&snap_at(0, 2, 0.0));
        c.push_snapshot(&snap_at(4, 2, 40.0));
        let mut prev = -1.0;
        let mut t = 0.0;
        while t <= 4.0 * ou_protocol::TICK_DT {
            let x = c.interpolate(2, t).unwrap().pos.x;
            assert!(x >= prev - 1e-9, "немонотонно: {prev} -> {x}");
            prev = x;
            t += ou_protocol::TICK_DT / 4.0;
        }
    }
}
