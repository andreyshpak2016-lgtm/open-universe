//! # ou-sim
//!
//! Детерминированная симуляция мира Open Universe.
//!
//! Здесь живут правила движения и шаг мира. Один и тот же код используют
//! авторитетный сервер и клиентский предсказатель, поэтому вычисления обязаны
//! быть **бит-в-бит воспроизводимыми**.
//!
//! ## Почему своя тригонометрия
//! `f64::sin()/cos()` из `libm` дают разный результат на разных платформах и
//! версиях библиотеки. Для lockstep/reconciliation это недопустимо. Поэтому
//! ниже — `det_sin`/`det_cos`: приведение диапазона + полином Тейлора, только
//! операции `+ - * /`, которые IEEE‑754 задаёт однозначно. `sqrt` использовать
//! можно: стандарт требует корректного округления.

use ou_protocol::{EntityState, InputFrame, NetId, Vec3, TICK_DT};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Детерминированная тригонометрия
// ---------------------------------------------------------------------------

/// Число π (константа, одинакова везде).
pub const PI: f64 = 3.141592653589793;
const TWO_PI: f64 = 6.283185307179586;

/// Приведение угла к диапазону [−π, π] только арифметическими операциями.
#[inline]
pub fn wrap_pi(mut x: f64) -> f64 {
    // round() — детерминированная операция IEEE‑754.
    x -= TWO_PI * (x / TWO_PI).round();
    // Страховка от края диапазона.
    if x > PI {
        x -= TWO_PI;
    } else if x < -PI {
        x += TWO_PI;
    }
    x
}

/// Детерминированный синус: полином Тейлора 13‑й степени после приведения.
pub fn det_sin(x: f64) -> f64 {
    let x = wrap_pi(x);
    let x2 = x * x;
    // Только + - * /, коэффициенты фиксированы.
    let mut term = x; // x^1/1!
    let mut sum = x;
    term = -term * x2 / 6.0; // -x^3/3!
    sum += term;
    term = -term * x2 / 20.0; // +x^5/5!  (3!*4*5)
    sum += term;
    term = -term * x2 / 42.0; // -x^7/7!  (5!*6*7)
    sum += term;
    term = -term * x2 / 72.0; // +x^9/9!
    sum += term;
    term = -term * x2 / 110.0; // -x^11/11!
    sum += term;
    term = -term * x2 / 156.0; // +x^13/13!
    sum += term;
    term = -term * x2 / 210.0; // -x^15/15!
    sum += term;
    term = -term * x2 / 272.0; // +x^17/17!
    sum += term;
    sum
}

/// Детерминированный косинус через сдвиг синуса.
pub fn det_cos(x: f64) -> f64 {
    det_sin(x + PI / 2.0)
}

// ---------------------------------------------------------------------------
// Модель движения
// ---------------------------------------------------------------------------

/// Параметры кинематики корабля (одинаковы на клиенте и сервере!).
#[derive(Clone, Copy, Debug)]
pub struct ShipParams {
    /// Максимальное ускорение тяги, ед/с².
    pub max_thrust: f64,
    /// Максимальная угловая скорость рыскания, рад/с.
    pub max_yaw_rate: f64,
    /// Максимальная угловая скорость тангажа, рад/с.
    pub max_pitch_rate: f64,
    /// Линейное демпфирование (вязкое трение), 1/с.
    pub linear_damping: f64,
    /// Ограничение модуля скорости, ед/с.
    pub max_speed: f64,
}

impl Default for ShipParams {
    fn default() -> Self {
        ShipParams {
            max_thrust: 45.0,
            max_yaw_rate: 1.2,
            max_pitch_rate: 1.0,
            linear_damping: 0.02,
            max_speed: 400.0,
        }
    }
}

/// Ньютоновская модель полёта в вакууме.
///
/// Детерминизм: только `+ - * /`, `sqrt` (детерминирован), `det_sin/det_cos`.
pub fn integrate(state: &mut EntityState, input: &InputFrame, p: &ShipParams, dt: f64) {
    // Углы хранятся как f32 для провода, но вычисляются в f64.
    let yaw0 = state.yaw as f64;
    let pitch0 = state.pitch as f64;

    // 1. Углы.
    let yaw = wrap_pi(yaw0 + input.yaw as f64 * p.max_yaw_rate * dt);
    let mut pitch = wrap_pi(pitch0 + input.pitch as f64 * p.max_pitch_rate * dt);
    // Ограничение тангажа «полюсами».
    const PITCH_LIMIT: f64 = PI / 2.0 - 0.02;
    if pitch > PITCH_LIMIT {
        pitch = PITCH_LIMIT;
    } else if pitch < -PITCH_LIMIT {
        pitch = -PITCH_LIMIT;
    }
    state.yaw = yaw as f32;
    state.pitch = pitch as f32;

    // 2. Направление «вперёд» из углов (своя тригонометрия).
    let cp = det_cos(pitch);
    let sp = det_sin(pitch);
    let cy = det_cos(yaw);
    let sy = det_sin(yaw);
    let forward = Vec3::new(cp * sy, sp, cp * cy);

    // 3. Тяга + демпфирование.
    let thrust = forward.scale(input.throttle as f64 * p.max_thrust);
    let damp = state.vel.scale(-p.linear_damping);
    state.vel = state.vel.add(thrust.add(damp).scale(dt));

    // 4. Ограничение скорости.
    let speed = state.vel.length();
    if speed > p.max_speed {
        state.vel = state.vel.scale(p.max_speed / speed);
    }

    // 5. Позиция.
    state.pos = state.pos.add(state.vel.scale(dt));
}

// ---------------------------------------------------------------------------
// Мир
// ---------------------------------------------------------------------------

/// Сущность мира.
#[derive(Clone, Debug)]
pub struct Entity {
    pub state: EntityState,
    /// Последний применённый сервером ввод (для подтверждения клиенту).
    pub last_input_seq: u32,
}

/// Мир с детерминированным порядком перебора (BTreeMap).
#[derive(Clone, Debug, Default)]
pub struct World {
    pub tick: u64,
    pub entities: BTreeMap<NetId, Entity>,
    pub params: ShipParams,
}

impl World {
    pub fn new() -> Self {
        World { tick: 0, entities: BTreeMap::new(), params: ShipParams::default() }
    }

    pub fn spawn(&mut self, id: NetId, pos: Vec3) -> NetId {
        self.entities.insert(
            id,
            Entity {
                state: EntityState {
                    id,
                    pos,
                    vel: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.0,
                },
                last_input_seq: 0,
            },
        );
        id
    }

    /// Один детерминированный шаг мира.
    pub fn step(&mut self, inputs: &BTreeMap<NetId, InputFrame>) {
        for (id, ent) in self.entities.iter_mut() {
            if let Some(input) = inputs.get(id) {
                integrate(&mut ent.state, input, &self.params, TICK_DT);
                ent.last_input_seq = input.seq;
            } else {
                // Без ввода — только инерция (демпфирование).
                let neutral = InputFrame::neutral(ent.last_input_seq);
                integrate(&mut ent.state, &neutral, &self.params, TICK_DT);
            }
        }
        self.tick += 1;
    }

    pub fn snapshot_map(&self) -> BTreeMap<NetId, EntityState> {
        self.entities.iter().map(|(k, v)| (*k, v.state)).collect()
    }

    /// Контрольная сумма для проверки детерминизма.
    pub fn checksum(&self) -> u64 {
        ou_protocol::state_checksum(&self.snapshot_map())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_matches_known_values() {
        assert!(det_sin(0.0).abs() < 1e-12);
        assert!((det_cos(0.0) - 1.0).abs() < 1e-9);
        assert!((det_sin(PI / 2.0) - 1.0).abs() < 1e-7);
        assert!(det_sin(PI).abs() < 1e-7);
        assert!((det_cos(PI) + 1.0).abs() < 1e-7);
        // Точность полинома на [-π, π] (ошибка у границ ~2e-8).
        let mut x = -PI;
        while x <= PI {
            let approx = det_sin(x);
            let exact = x.sin();
            assert!((approx - exact).abs() < 1e-7, "sin({x}) err={}", (approx - exact).abs());
            x += 0.05;
        }
    }

    #[test]
    fn wrap_pi_range() {
        // 3π приводится к ±π (это один и тот же угол).
        assert!((wrap_pi(3.0 * PI).abs() - PI).abs() < 1e-9);
        assert!(wrap_pi(100.0) <= PI + 1e-9);
        assert!(wrap_pi(-100.0) >= -PI - 1e-9);
    }

    #[test]
    fn throttle_accelerates_along_forward() {
        let mut w = World::new();
        w.spawn(1, Vec3::ZERO);
        let mut inputs = BTreeMap::new();
        inputs.insert(1, InputFrame { seq: 1, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false });
        for _ in 0..30 {
            w.step(&inputs);
        }
        let e = w.entities.get(&1).unwrap();
        // yaw=0, pitch=0 → «вперёд» это +Z.
        assert!(e.state.pos.z > 1.0, "pos.z = {}", e.state.pos.z);
        assert!(e.state.pos.x.abs() < 1e-9);
        assert!(e.state.vel.z > 0.0);
    }

    #[test]
    fn determinism_same_inputs_same_checksum() {
        let run = |seed: u64| {
            let mut w = World::new();
            for i in 0..8 {
                w.spawn(i as NetId, Vec3::new(i as f64 * 10.0, 0.0, 0.0));
            }
            for t in 0..600u32 {
                let mut inputs = BTreeMap::new();
                for i in 0..8 {
                    let id = i as NetId;
                    // Псевдо-случайный, но полностью детерминированный ввод.
                    let s = (seed.wrapping_mul(31).wrapping_add(t as u64).wrapping_add(id as u64)) as f64;
                    let throttle = det_sin(s) as f32;
                    let yaw = det_cos(s * 0.5) as f32;
                    inputs.insert(id, InputFrame { seq: t, throttle, yaw, pitch: 0.2, fire: false });
                }
                w.step(&inputs);
            }
            w.checksum()
        };
        assert_eq!(run(1), run(1));
        assert_ne!(run(1), run(2));
    }

    #[test]
    fn no_input_keeps_inertia_and_damps() {
        let mut w = World::new();
        w.spawn(1, Vec3::ZERO);
        let mut inputs = BTreeMap::new();
        inputs.insert(1, InputFrame { seq: 1, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false });
        for _ in 0..30 {
            w.step(&inputs);
        }
        let v_before = w.entities.get(&1).unwrap().state.vel.length();
        let empty = BTreeMap::new();
        for _ in 0..300 {
            w.step(&empty);
        }
        let v_after = w.entities.get(&1).unwrap().state.vel.length();
        assert!(v_after < v_before, "демпфирование должно уменьшать скорость");
    }
}
