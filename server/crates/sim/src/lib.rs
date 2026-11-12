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

use ou_protocol::{EntityState, InputFrame, NetId, TICK_DT};
use std::collections::BTreeMap;

pub mod terrain;

/// Реэкспорт базовых типов, чтобы потребителям хватало одного крейта.
pub use ou_protocol::Vec3;
pub use terrain::{landing, normalize, Terrain, TerrainParams};

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
    /// Скорость, до которой посадка считается мягкой, ед/с.
    pub landing_safe_speed: f64,
    /// Трение о поверхность (0..1): доля касательной скорости, теряемая за шаг.
    pub ground_friction: f64,
}

impl Default for ShipParams {
    fn default() -> Self {
        ShipParams {
            max_thrust: 45.0,
            max_yaw_rate: 1.2,
            max_pitch_rate: 1.0,
            linear_damping: 0.02,
            max_speed: 400.0,
            landing_safe_speed: 60.0,
            ground_friction: 0.25,
        }
    }
}

/// Ускорение свободного падения у поверхности планеты, ед/с².
///
/// Меньше тяги двигателя (`max_thrust`), чтобы корабль мог оторваться от
/// поверхности, но гравитация при этом заметна.
pub const SURFACE_GRAVITY: f64 = 18.0;

/// До какого расстояния от центра планеты действует гравитация (в радиусах).
/// Дальше — космос: поле почти нулевое, полёт инерционный.
pub const GRAVITY_RANGE_RADII: f64 = 30.0;

/// Гравитационное ускорение в точке `pos` от планеты `terrain` (0 — космос).
///
/// Модель обратных квадратов: `g = g0·(R/r)²`, где `R` — радиус планеты.
/// Это даёт «планетную» физику у поверхности и почти нулевую — вдали, то есть
/// космический инерционный полёт без смены кода.
pub fn gravity_at(pos: Vec3, terrain: &Terrain) -> Vec3 {
    let rel = pos.sub(terrain.center);
    let dist = rel.length();
    if dist < 1e-6 {
        return Vec3::ZERO;
    }
    let surface = terrain.params.radius;
    if dist > surface * GRAVITY_RANGE_RADII {
        return Vec3::ZERO; // далеко от планеты — космос
    }
    let up = rel.scale(1.0 / dist);
    // Не даём ускорению «взрываться» у центра: считаем от поверхности.
    let r = dist.max(surface);
    let ratio = surface / r;
    let g = SURFACE_GRAVITY * ratio * ratio;
    up.scale(-g)
}

/// Ньютоновская модель полёта с внешним ускорением (гравитация планеты).
///
/// Детерминизм: только `+ - * /`, `sqrt` (детерминирован), `det_sin/det_cos`.
/// `external_accel` — ускорение, не зависящее от корабля (гравитация); для
/// открытого космоса передавайте [`Vec3::ZERO`].
pub fn integrate_with_accel(
    state: &mut EntityState,
    input: &InputFrame,
    p: &ShipParams,
    dt: f64,
    external_accel: Vec3,
) {
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

    // 3. Тяга + демпфирование + гравитация.
    let thrust = forward.scale(input.throttle as f64 * p.max_thrust);
    let damp = state.vel.scale(-p.linear_damping);
    state.vel = state
        .vel
        .add(thrust.add(damp).add(external_accel).scale(dt));

    // 4. Ограничение скорости.
    let speed = state.vel.length();
    if speed > p.max_speed {
        state.vel = state.vel.scale(p.max_speed / speed);
    }

    // 5. Позиция.
    state.pos = state.pos.add(state.vel.scale(dt));
}

/// Ньютоновская модель полёта в вакууме (без гравитации).
///
/// Детерминизм: только `+ - * /`, `sqrt` (детерминирован), `det_sin/det_cos`.
pub fn integrate(state: &mut EntityState, input: &InputFrame, p: &ShipParams, dt: f64) {
    integrate_with_accel(state, input, p, dt, Vec3::ZERO);
}

// ---------------------------------------------------------------------------
// Домашняя планета: общий мир для сервера и клиента
// ---------------------------------------------------------------------------

/// Радиус домашней планеты в игровых единицах (1 ед. = 1 м).
///
/// 8000 ед. = 8 км: горизонт у поверхности уходит на десятки километров,
/// поэтому планета читается как «земля», а не как маленький шар перед камерой.
pub const HOME_PLANET_RADIUS: f64 = 8000.0;
/// Размах рельефа домашней планеты.
pub const HOME_PLANET_AMPLITUDE: f64 = 60.0;
/// Семя рельефа (совпадает у сервера и клиента).
pub const HOME_PLANET_SEED: u64 = 0xC0FFEE;

/// Домашняя планета: точка появления игроков. Центр — в начале координат мира.
pub fn home_planet() -> Terrain {
    Terrain::new(
        Vec3::ZERO,
        TerrainParams {
            radius: HOME_PLANET_RADIUS,
            amplitude: HOME_PLANET_AMPLITUDE,
            sea_level: 0.30,
            seed: HOME_PLANET_SEED,
        },
    )
}

/// Направление точки появления: первое «сухое» направление на поверхности.
///
/// Ищем детерминированно (фиксированный обход), чтобы сервер и клиент
/// поставили корабль в **одну и ту же точку**. Точка выбирается на суше
/// (высота выше уровня моря), чтобы игрок видел ландшафт, а не океан.
pub fn home_spawn_direction() -> Vec3 {
    let t = home_planet();
    // Обход по сфере фиксированным шагом; результат не зависит от платформы.
    const STEP: f64 = 0.15;
    let mut phi = -1.2_f64;
    while phi <= 1.2 {
        let mut theta = 0.0_f64;
        while theta < 2.0 * PI {
            let d = Vec3::new(
                det_cos(phi) * det_cos(theta),
                det_sin(phi),
                det_cos(phi) * det_sin(theta),
            );
            // Хотим заметную сушу, но не самую вершину горы.
            let h = t.height(d);
            if h > t.params.amplitude * 0.15 && h < t.params.amplitude * 0.9 {
                return normalize(d);
            }
            theta += STEP;
        }
        phi += STEP;
    }
    // Запасной вариант: «вверх».
    Vec3::new(0.0, 1.0, 0.0)
}

/// Точка появления над поверхностью домашней планеты (на `clearance` над землёй).
pub fn home_spawn(clearance: f64) -> Vec3 {
    let t = home_planet();
    let dir = home_spawn_direction();
    let surface = t.surface_radius(dir);
    t.center.add(dir.scale(surface + clearance))
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
    /// Планета, с которой взаимодействует сущность (посадка, столкновения).
    pub terrain: Option<Terrain>,
    /// Состояние контакта с поверхностью после последнего шага.
    pub ground: landing::GroundState,
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
                terrain: None,
                ground: landing::GroundState::Flying,
            },
        );
        id
    }

    /// Один детерминированный шаг мира.
    pub fn step(&mut self, inputs: &BTreeMap<NetId, InputFrame>) {
        for (id, ent) in self.entities.iter_mut() {
            // Внешнее ускорение: гравитация планеты, если сущность к ней привязана.
            let g = match ent.terrain {
                Some(t) => gravity_at(ent.state.pos, &t),
                None => Vec3::ZERO,
            };
            if let Some(input) = inputs.get(id) {
                integrate_with_accel(&mut ent.state, input, &self.params, TICK_DT, g);
                ent.last_input_seq = input.seq;
            } else {
                // Без ввода — только инерция (демпфирование) и гравитация.
                let neutral = InputFrame::neutral(ent.last_input_seq);
                integrate_with_accel(&mut ent.state, &neutral, &self.params, TICK_DT, g);
            }
            // Столкновение с рельефом: если у сущности есть планета, корабль
            // не может «провалиться» под поверхность.
            if let Some(terrain) = ent.terrain {
                let state = ent.state;
                let mut pos = state.pos;
                let mut vel = state.vel;
                let res = landing::resolve_ground(
                    &mut pos,
                    &mut vel,
                    &terrain,
                    self.params.landing_safe_speed,
                    self.params.ground_friction,
                    &self.params,
                );
                ent.state.pos = pos;
                ent.state.vel = vel;
                ent.ground = res;
            }
        }
        self.tick += 1;
    }

    /// Привязать сущность к планете (для посадки и столкновений).
    pub fn attach_terrain(&mut self, id: NetId, terrain: Terrain) {
        if let Some(e) = self.entities.get_mut(&id) {
            e.terrain = Some(terrain);
        }
    }

    pub fn snapshot_map(&self) -> BTreeMap<NetId, EntityState> {
        self.entities.iter().map(|(k, v)| (*k, v.state)).collect()
    }

    /// Контрольная сумма для проверки детерминизма.
    pub fn checksum(&self) -> u64 {
        ou_protocol::state_checksum(&self.snapshot_map())
    }
}

// ---------------------------------------------------------------------------
// Генерация мира: детерминированные звёздные системы
// ---------------------------------------------------------------------------

/// Детерминированный генератор псевдослучайных чисел (SplitMix64).
///
/// Используется вместо `rand`, потому что генерация мира обязана быть
/// воспроизводимой из одного числа‑семени на любой платформе.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64, salt: u64) -> Self {
        // Смешиваем семя и соль, чтобы разные подсистемы давали разные потоки.
        Rng { state: seed ^ salt.wrapping_mul(0x9E3779B97F4A7C15) }
    }

    /// Следующее 64-битное число.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Значение в диапазоне [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Значение в диапазоне [lo, hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    /// Целое в диапазоне [lo, hi].
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
}

/// Тип небесного тела.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Star,
    Planet,
    Moon,
    GasGiant,
    AsteroidBelt,
    BlackHole,
}

/// Небесное тело в сгенерированной системе.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub id: u32,
    pub kind: BodyKind,
    /// Позиция относительно центра системы (для чёрных дыр — сама сингулярность).
    pub pos: Vec3,
    pub radius: f64,
    /// Радиус орбиты (0 для центрального тела).
    pub orbit: f64,
    /// Орбитальная фаза в радианах.
    pub phase: f64,
    /// Количество ресурсов (для поясов астероидов), условных единиц.
    pub resources: u64,
}

/// Сгенерированная звёздная система.
#[derive(Clone, Debug, PartialEq)]
pub struct StarSystem {
    pub id: u32,
    pub name: String,
    pub seed: u64,
    pub pos: Vec3,
    pub bodies: Vec<Body>,
}

impl StarSystem {
    pub fn body(&self, id: u32) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// Суммарные ресурсы всех поясов астероидов.
    pub fn total_resources(&self) -> u64 {
        self.bodies.iter().map(|b| b.resources).sum()
    }

    /// Позиция тела в момент `t` (сек) по круговой орбите. Детерминирована.
    pub fn body_position_at(&self, id: u32, t: f64) -> Option<Vec3> {
        let b = self.body(id)?;
        if b.orbit == 0.0 {
            return Some(b.pos); // центральное тело неподвижно
        }
        // Угловая скорость ~ 1/sqrt(orbit) — упрощённый закон Кеплера.
        let omega = 1.0 / b.orbit.sqrt();
        let a = b.phase + omega * t;
        // Орбита отсчитывается от центра системы (self.pos), а не от b.pos.
        Some(Vec3::new(
            self.pos.x + b.orbit * det_cos(a),
            b.pos.y,
            self.pos.z + b.orbit * det_sin(a),
        ))
    }
}

const SYLLABLES: [&str; 12] = [
    "Al", "Be", "Cor", "Dra", "El", "Fen", "Gly", "Hy", "Ix", "Kry", "Lu", "Ny",
];
const SUFFIXES: [&str; 8] = ["ra", "tis", "on", "us", "ea", "ar", "ov", "ium"];

/// Сгенерировать имя системы из семени (детерминированно).
pub fn system_name(id: u32, seed: u64) -> String {
    let mut rng = Rng::new(seed, 0xA11CE);
    let a = SYLLABLES[(rng.next_u64() % SYLLABLES.len() as u64) as usize];
    let b = SUFFIXES[(rng.next_u64() % SUFFIXES.len() as u64) as usize];
    format!("{a}{b}-{id}")
}

/// Сгенерировать звёздную систему по семени.
///
/// Детерминированно: одно и то же `seed` даёт одну и ту же систему.
pub fn generate_system(id: u32, seed: u64, center: Vec3) -> StarSystem {
    let mut rng = Rng::new(seed, 0x5EED);

    // Решаем, что в центре: звезда или чёрная дыра (редко).
    let central_kind = if rng.next_f64() < 0.12 { BodyKind::BlackHole } else { BodyKind::Star };
    let central_radius = match central_kind {
        BodyKind::BlackHole => rng.range(8.0, 25.0),
        _ => rng.range(200.0, 900.0),
    };
    let mut bodies = vec![Body {
        id: 0,
        kind: central_kind,
        pos: center,
        radius: central_radius,
        orbit: 0.0,
        phase: 0.0,
        resources: 0,
    }];

    // Планеты и газовые гиганты.
    let planet_count = rng.range_i64(2, 8) as u32;
    let mut orbit = central_radius * 3.0 + rng.range(500.0, 1500.0);
    for i in 0..planet_count {
        let gas = rng.next_f64() < 0.3;
        let kind = if gas { BodyKind::GasGiant } else { BodyKind::Planet };
        let radius = if gas { rng.range(500.0, 1200.0) } else { rng.range(80.0, 260.0) };
        let id = 1 + i;
        bodies.push(Body {
            id,
            kind,
            pos: Vec3::new(center.x + orbit, center.y, center.z),
            radius,
            orbit,
            phase: rng.range(0.0, TWO_PI),
            resources: if gas { 0 } else { rng.range_i64(50, 400) as u64 },
        });
        // Луны для планет (не газовых).
        if !gas && rng.next_f64() < 0.5 {
            let moon_orbit = radius * 4.0;
            bodies.push(Body {
                id: 100 + i,
                kind: BodyKind::Moon,
                pos: Vec3::new(center.x + orbit, center.y, center.z),
                radius: radius * rng.range(0.15, 0.35),
                orbit: moon_orbit,
                phase: rng.range(0.0, TWO_PI),
                resources: rng.range_i64(5, 40) as u64,
            });
        }
        // Раздвигаем орбиты с запасом.
        orbit += radius * 6.0 + rng.range(800.0, 2500.0);
    }

    // Пояса астероидов — источники ресурсов.
    let belt_count = rng.range_i64(1, 3) as u32;
    for i in 0..belt_count {
        let belt_orbit = rng.range(orbit, orbit * 1.8);
        bodies.push(Body {
            id: 200 + i,
            kind: BodyKind::AsteroidBelt,
            pos: Vec3::new(center.x + belt_orbit, center.y, center.z),
            radius: rng.range(200.0, 600.0),
            orbit: belt_orbit,
            phase: rng.range(0.0, TWO_PI),
            resources: rng.range_i64(500, 5000) as u64,
        });
    }

    StarSystem {
        id,
        name: system_name(id, seed),
        seed,
        pos: center,
        bodies,
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

    // --- Генерация мира ---

    #[test]
    fn rng_is_reproducible() {
        let mut a = Rng::new(42, 7);
        let mut b = Rng::new(42, 7);
        let mut c = Rng::new(43, 7);
        let seq_a: Vec<u64> = (0..10).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..10).map(|_| b.next_u64()).collect();
        let first_c = c.next_u64();
        assert_eq!(seq_a, seq_b)
        ;
        assert_ne!(seq_a[0], first_c);
    }

    #[test]
    fn rng_range_bounds() {
        let mut r = Rng::new(1, 2);
        for _ in 0..1000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
            let x = r.range(-5.0, 5.0);
            assert!((-5.0..5.0).contains(&x));
            let i = r.range_i64(3, 7);
            assert!((3..=7).contains(&i));
        }
    }

    #[test]
    fn system_generation_is_deterministic() {
        let a = generate_system(1, 999, Vec3::ZERO);
        let b = generate_system(1, 999, Vec3::ZERO);
        let c = generate_system(1, 1000, Vec3::ZERO);
        assert_eq!(a, b, "одно семя должно давать одинаковую систему");
        assert_ne!(a, c, "разные семена дают разные системы");
    }

    #[test]
    fn system_has_star_and_bodies() {
        let s = generate_system(5, 12345, Vec3::new(1000.0, 0.0, 0.0));
        // Центральное тело присутствует и находится в центре системы.
        let center = s.body(0).unwrap();
        assert!(matches!(center.kind, BodyKind::Star | BodyKind::BlackHole));
        assert_eq!(center.pos, Vec3::new(1000.0, 0.0, 0.0));
        // Есть планеты и хотя бы один пояс астероидов с ресурсами.
        assert!(s.bodies.iter().any(|b| b.kind == BodyKind::Planet || b.kind == BodyKind::GasGiant));
        assert!(s.bodies.iter().any(|b| b.kind == BodyKind::AsteroidBelt));
        assert!(s.total_resources() > 0);
    }

    #[test]
    fn orbit_is_periodic_and_bounded() {
        let s = generate_system(2, 777, Vec3::ZERO);
        let asteroid = s.bodies.iter().find(|b| b.kind == BodyKind::AsteroidBelt).unwrap();
        let r = asteroid.orbit;
        // Тело всегда на расстоянии r от центра системы.
        for t in [0.0, 10.0, 100.0, 1234.5] {
            let p = s.body_position_at(asteroid.id, t).unwrap();
            let d = Vec3::new(p.x - s.pos.x, 0.0, p.z - s.pos.z).length();
            // Радиус орбиты большой, а тригонометрия полиномиальная: допуск
            // задаём относительно r (ошибка ~1e-8, умноженная на r).
            assert!((d - r).abs() < r * 1e-6, "t={t} d={d} r={r}");
        }
    }

    // --- Домашняя планета и гравитация ---

    #[test]
    fn home_spawn_is_on_land_and_repeatable() {
        let a = home_spawn_direction();
        let b = home_spawn_direction();
        assert_eq!(a.to_wire(), b.to_wire(), "точка появления должна быть неизменной");
        let t = home_planet();
        // Точка появления — над сушей (иначе игрок рождается в океане).
        assert!(t.height(a) > 0.0, "точка появления должна быть на суше");
        let p = home_spawn(50.0);
        // Над поверхностью, но не слишком высоко.
        let dist = p.length();
        assert!(dist > t.params.radius, "точка появления над поверхностью");
        assert!(dist < t.params.radius + t.params.amplitude * 2.0 + 100.0);
    }

    #[test]
    fn gravity_pulls_toward_planet_and_zero_in_space() {
        let t = home_planet();
        // У поверхности гравитация направлена к центру и заметна.
        let dir = normalize(Vec3::new(0.2, 0.9, -0.1));
        let surface = t.surface_radius(dir);
        let pos = t.center.add(dir.scale(surface + 10.0));
        let g = gravity_at(pos, &t);
        assert!(g.length() > 1.0, "у поверхности гравитация должна быть заметной");
        // Направление — внутрь (против «вверх»).
        assert!(g.dot(dir) < 0.0, "гравитация тянет к центру планеты");
        // Далеко в космосе — нулевая.
        let far = t.center.add(dir.scale(t.params.radius * (GRAVITY_RANGE_RADII + 5.0)));
        assert_eq!(gravity_at(far, &t), Vec3::ZERO);
    }

    #[test]
    fn ship_falls_under_gravity_without_input() {
        let mut w = World::new();
        let t = home_planet();
        let dir = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surface = t.surface_radius(dir);
        let start = t.center.add(dir.scale(surface + 400.0));
        let id = w.spawn(1, start);
        w.attach_terrain(id, t);
        let empty = BTreeMap::new();
        for _ in 0..60 {
            w.step(&empty);
        }
        let e = w.entities.get(&1).unwrap();
        // Корабль приблизился к центру планеты (упал).
        assert!(e.state.pos.length() < start.length(), "корабль должен падать");
    }

    #[test]
    fn space_flight_is_inertial_without_gravity() {
        // Без планеты (terrain = None) полёт инерционный: тяга разгоняет,
        // затем корабль летит по прямой.
        let mut w = World::new();
        w.spawn(1, Vec3::ZERO);
        let mut inputs = BTreeMap::new();
        inputs.insert(1, InputFrame { seq: 1, throttle: 1.0, yaw: 0.0, pitch: 0.0, fire: false });
        for _ in 0..30 {
            w.step(&inputs);
        }
        let z_after_thrust = w.entities.get(&1).unwrap().state.pos.z;
        let empty = BTreeMap::new();
        for _ in 0..30 {
            w.step(&empty);
        }
        let z_after_coast = w.entities.get(&1).unwrap().state.pos.z;
        assert!(z_after_coast > z_after_thrust, "в космосе корабль летит по инерции");
    }
}
