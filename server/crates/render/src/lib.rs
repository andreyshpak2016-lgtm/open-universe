//! # ou-render
//!
//! Софтверный рендерер Open Universe: настоящая трассировка лучей на CPU,
//! без GPU и без внешних библиотек.
//!
//! ## Зачем
//! Наш клиент по ADR-0001 — Unreal Engine 5. Но пока UE не установлен, а вся
//! *логика* мира (позиции, ориентации, снапшоты) уже есть на сервере, нужен
//! способ **увидеть** этот мир. Этот крейт рендерит сцену из того же состояния,
//! что приходит по сети, и сохраняет кадр в PNG.
//!
//! Компоненты (камера, floating origin, сфера-планета, блочный корабль, звёзды)
//! переносятся в UE5 один-к-одному — математика здесь та же, что понадобится там.
//!
//! ## Что рендерится
//! * звёздное небо (детерминированное, из хеша направления),
//! * планета-сфера с процедурным шумом и освещением по Ламберту,
//! * корабль как набор блоков (OBB) — в духе воксельных конструкций,
//! * атмосферное свечение по краю планеты,
//! * источник света (звезда) с бликом.
//!
//! ## Производительность
//! Это учебный/отладочный рендер: он честно трассирует лучи, но не гоняется за
//! кадрами в секунду. Он даёт **референсный кадр** для сверки и скриншот.

pub mod png;

use ou_protocol::Vec3;
use ou_sim::{det_cos, det_sin};

// ---------------------------------------------------------------------------
// Математика
// ---------------------------------------------------------------------------

/// Нормализовать вектор.
fn norm(v: Vec3) -> Vec3 {
    let l = v.length();
    if l == 0.0 {
        Vec3::ZERO
    } else {
        v.scale(1.0 / l)
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Смешать два вектора.
#[allow(dead_code)]
fn lerp3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    a.add(b.sub(a).scale(t))
}

/// Смешать два цвета.
fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    a.add(b.add(a.scale(-1.0)).scale(t))
}

/// Хеш решётки в [0,1).
fn lattice_hash(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed;
    for c in [x, y, z] {
        h ^= (c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Плавная интерполяция (smoothstep).
fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Трилинейный value noise: узлы решётки сглаживаются, поэтому нет «клеток».
fn value_noise(p: Vec3, freq: f64, seed: u64) -> f64 {
    let x = p.x * freq;
    let y = p.y * freq;
    let z = p.z * freq;
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (smooth(x - xi), smooth(y - yi), smooth(z - zi));
    let (ix, iy, iz) = (xi as i64, yi as i64, zi as i64);

    let c = |dx: i64, dy: i64, dz: i64| lattice_hash(ix + dx, iy + dy, iz + dz, seed);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;

    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), fx);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), fx);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), fx);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), fx);
    let y0 = lerp(x00, x10, fy);
    let y1 = lerp(x01, x11, fy);
    lerp(y0, y1, fz)
}

/// Фрактальный шум (несколько октав) — похоже на поверхность планеты.
fn fbm(p: Vec3, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.5;
    let mut norm_factor = 0.0;
    for _ in 0..5 {
        sum += value_noise(p, freq, seed) * amp;
        norm_factor += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm_factor
}

// ---------------------------------------------------------------------------
// Сцена
// ---------------------------------------------------------------------------

/// Цвет в линейном пространстве 0..1.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Color {
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Color { r, g, b }
    }
    fn scale(self, k: f64) -> Self {
        Color::rgb(self.r * k, self.g * k, self.b * k)
    }
    fn add(self, o: Color) -> Self {
        Color::rgb(self.r + o.r, self.g + o.g, self.b + o.b)
    }
    #[allow(dead_code)]
    fn mul(self, o: Color) -> Self {
        Color::rgb(self.r * o.r, self.g * o.g, self.b * o.b)
    }
    fn to_rgb8(self) -> [u8; 3] {
        // Гамма-коррекция и ограничение.
        let g = |v: f64| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
        [g(self.r), g(self.g), g(self.b)]
    }
}

/// Ориентированный блок (деталь корабля).
#[derive(Clone, Debug)]
pub struct Block {
    pub center: Vec3,
    pub half: Vec3,
    /// Базовый цвет.
    pub color: Color,
}

impl Block {
    /// Пересечение луча с OBB. Возвращает расстояние, если попали.
    ///
    /// Работаем в мире, где блоки выровнены по осям (для корабля ориентация
    /// учитывается при построении). Так проще и достаточно для отладочного кадра.
    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3)> {
        let min = self.center.sub(self.half);
        let max = self.center.add(self.half);
        let mut tmin = f64::NEG_INFINITY;
        let mut tmax = f64::INFINITY;
        let axes = [
            (origin.x, dir.x, min.x, max.x),
            (origin.y, dir.y, min.y, max.y),
            (origin.z, dir.z, min.z, max.z),
        ];
        for (o, d, lo, hi) in axes {
            if d.abs() < 1e-9 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let t1 = (lo - o) / d;
                let t2 = (hi - o) / d;
                let (near, far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
                tmin = tmin.max(near);
                tmax = tmax.min(far);
                if tmin > tmax {
                    return None;
                }
            }
        }
        if tmax < 0.0 {
            return None;
        }
        let t = if tmin > 0.0 { tmin } else { tmax };
        let p = origin.add(dir.scale(t));
        Some((t, p))
    }
}

/// Сфера (планета или звезда).
#[derive(Clone, Debug)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f64,
    /// Цвет поверхности.
    pub color: Color,
    /// Есть ли процедурный рельеф (планета) или ровный цвет (звезда).
    pub terrain: bool,
    /// Собственное свечение (для звезды).
    pub emissive: f64,
    pub seed: u64,
    /// Толщина атмосферы (0 — нет).
    pub atmosphere: f64,
}

impl Sphere {
    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3, Vec3)> {
        let oc = origin.sub(self.center);
        let b = dot(oc, dir);
        let c = dot(oc, oc) - self.radius * self.radius;
        let disc = b * b - c;
        if disc < 0.0 {
            return None;
        }
        let sq = disc.sqrt();
        let t = if -b - sq > 0.0 { -b - sq } else { -b + sq };
        if t < 0.0 {
            return None;
        }
        let p = origin.add(dir.scale(t));
        Some((t, p, p.sub(self.center)))
    }
}

/// Камера.
#[derive(Clone, Debug)]
pub struct Camera {
    pub origin: Vec3,
    /// Куда смотрит.
    pub forward: Vec3,
    pub up: Vec3,
    /// Поле зрения по вертикали (радианы).
    pub fov: f64,
}

impl Camera {
    /// Луч для пикселя (nx, ny ∈ [0,1]).
    fn ray(&self, nx: f64, ny: f64, aspect: f64) -> Vec3 {
        let f = norm(self.forward);
        let r = norm(cross(f, self.up));
        let u = cross(r, f);
        let half_h = (self.fov * 0.5).tan();
        let half_w = half_h * aspect;
        let px = (nx * 2.0 - 1.0) * half_w;
        let py = (1.0 - ny * 2.0) * half_h;
        norm(f.add(r.scale(px)).add(u.scale(py)))
    }
}

/// Освещение.
#[derive(Clone, Debug)]
pub struct Light {
    pub direction: Vec3,
    pub color: Color,
    /// Направление, куда светит (нормализовано к источнику).
    pub intensity: f64,
}

/// Целиком сцена.
#[derive(Clone, Debug)]
pub struct Scene {
    pub camera: Camera,
    pub spheres: Vec<Sphere>,
    pub blocks: Vec<Block>,
    pub light: Light,
    /// Цвет глубокого космоса.
    pub space: Color,
    pub star_density: f64,
    pub star_seed: u64,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            camera: Camera {
                origin: Vec3::new(0.0, 0.0, 0.0),
                forward: Vec3::new(0.0, 0.0, 1.0),
                up: Vec3::new(0.0, 1.0, 0.0),
                fov: 1.2,
            },
            spheres: Vec::new(),
            blocks: Vec::new(),
            light: Light {
                direction: Vec3::new(0.5, 0.8, 0.3),
                color: Color::rgb(1.0, 0.96, 0.88),
                intensity: 1.2,
            },
            space: Color::rgb(0.01, 0.012, 0.03),
            star_density: 0.12,
            star_seed: 0xC0FFEE,
        }
    }
}

// ---------------------------------------------------------------------------
// Трассировка
// ---------------------------------------------------------------------------

/// Фон: звёзды точками, туманность и «галактическая полоса».
fn background(dir: Vec3, scene: &Scene) -> Color {
    let d = norm(dir);
    let mut col = scene.space;

    // Звёзды: решётка направлений; в редких ячейках ставим точку и рисуем
    // её как маленький гауссов ореол, поэтому звёзды круглые, а не квадратные.
    let cell = 90.0;
    let xf = d.x * cell;
    let yf = d.y * cell;
    let zf = d.z * cell;
    let (xi, yi, zi) = (xf.floor() as i64, yf.floor() as i64, zf.floor() as i64);
    let f = (xf - xi as f64, yf - yi as f64, zf - zi as f64);

    for dx in -1..=1i64 {
        for dy in -1..=1i64 {
            for dz in -1..=1i64 {
                let cx = xi + dx;
                let cy = yi + dy;
                let cz = zi + dz;
                let exist = lattice_hash(cx, cy, cz, scene.star_seed);
                if exist >= scene.star_density {
                    continue;
                }
                // Смещение звезды внутри ячейки.
                let ox = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x11);
                let oy = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x22);
                // Положение звезды в пространстве ячеек.
                let sx = cx as f64 + ox - xf;
                let sy = cy as f64 + oy - yf;
                let sz = cz as f64 - zf;
                let dist2 = sx * sx + sy * sy + sz * sz;
                // Ореол: чем ближе к точке, тем ярче.
                let bright = ((0.16 - dist2) / 0.16).max(0.0);
                if bright <= 0.0 {
                    continue;
                }
                let mag = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x33);
                let intensity = (0.4 + mag * 1.8) * bright * bright * 2.2;
                let warm = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x44);
                col = col.add(Color::rgb(
                    intensity * (0.8 + 0.3 * warm),
                    intensity,
                    intensity * (1.15 - 0.3 * warm),
                ));
            }
        }
    }

    // Туманность: низкочастотный шум по направлению.
    let n = fbm(Vec3::new(d.x * 2.2, d.y * 2.2, d.z * 2.2), scene.star_seed ^ 0xABCD);
    if n > 0.55 {
        let t = ((n - 0.55) / 0.45).min(1.0);
        let neb = Color::rgb(0.10, 0.05, 0.24).add(Color::rgb(0.06, 0.03, 0.14).scale(t));
        col = lerp_color(col, neb, t * 0.6);
    }

    // Галактическая полоса — мягкая светлая полоса поперёк неба.
    let band = (d.y * 3.2).abs();
    if band < 0.5 {
        let t = 1.0 - band / 0.5;
        col = col.add(Color::rgb(0.018, 0.02, 0.035).scale(t * t));
    }
    col
}

/// Осветить точку поверхности: ambient + diffuse + блик.
fn shade(base: Color, n: Vec3, view: Vec3, ldir: Vec3, light: &Light, specular: f64) -> Color {
    let ndl = dot(n, ldir).max(0.0);
    let ambient = 0.06;
    let mut col = base.mul(light.color).scale((ambient + (1.0 - ambient) * ndl) * light.intensity);

    // Блик (Блинн-Фонг).
    if specular > 0.0 && ndl > 0.0 {
        let h = norm(ldir.add(view));
        let ndh = dot(n, h).max(0.0);
        let spec = ndh.powf(48.0) * specular;
        col = col.add(light.color.scale(spec * light.intensity));
    }
    col
}

/// Трассировать один луч и вернуть цвет.
pub fn trace(scene: &Scene, origin: Vec3, dir: Vec3, aspect: f64) -> Color {
    let _ = aspect;
    let ldir = norm(scene.light.direction);
    let view = norm(dir.scale(-1.0));

    // Ближайшее пересечение.
    let mut best_t = f64::INFINITY;
    let mut best_color = background(dir, scene);
    let mut shaded = false;

    for s in &scene.spheres {
        if let Some((t, _p, rel)) = s.hit(origin, dir) {
            if t < best_t {
                best_t = t;
                if s.emissive > 0.0 {
                    // Собственное свечение (звезда).
                    best_color = s.color.scale(s.emissive);
                    shaded = true;
                } else {
                    let n = norm(rel);
                    // Рельеф: суша/океан по процедурному шуму.
                    let base = if s.terrain {
                        let land = fbm(rel.scale(1.0 / s.radius * 7.0), s.seed);
                        if land > 0.5 {
                            let t2 = ((land - 0.5) * 2.0).min(1.0);
                            lerp_color(
                                Color::rgb(0.14, 0.30, 0.16),
                                Color::rgb(0.44, 0.38, 0.24),
                                t2,
                            )
                        } else {
                            Color::rgb(0.03, 0.10, 0.26)
                        }
                    } else {
                        s.color
                    };
                    let mut col = shade(base, n, view, ldir, &scene.light, 0.05);
                    // Атмосферное свечение по краю (френель).
                    if s.atmosphere > 0.0 {
                        let fres = (1.0 - dot(n, view).abs()).powf(2.5);
                        let ndl = dot(n, ldir).max(0.0);
                        let atmo = Color::rgb(0.30, 0.58, 1.0)
                            .scale(fres * s.atmosphere * (0.25 + 0.9 * ndl));
                        col = col.add(atmo);
                    }
                    best_color = col;
                    shaded = true;
                }
            }
        }
    }

    // Блоки (детали корабля, астероиды).
    for b in &scene.blocks {
        if let Some((t, p)) = b.hit(origin, dir) {
            if t < best_t {
                best_t = t;
                // Нормаль по ближайшей грани OBB.
                let rel = p.sub(b.center);
                let ax = rel.x.abs() / b.half.x;
                let ay = rel.y.abs() / b.half.y;
                let az = rel.z.abs() / b.half.z;
                let n = if ax >= ay && ax >= az {
                    Vec3::new(rel.x.signum(), 0.0, 0.0)
                } else if ay >= az {
                    Vec3::new(0.0, rel.y.signum(), 0.0)
                } else {
                    Vec3::new(0.0, 0.0, rel.z.signum())
                };
                // Каждый блок чуть отличается оттенком — «воксельная» текстура.
                let tnoise = 0.9 + 0.25 * value_noise(p, 6.0, 0x7777);
                // У блочных поверхностей слабый ambient, иначе камни «тонут».
                let ndl = dot(n, ldir).max(0.0);
                let lit = b.color.scale(tnoise).scale(0.32 + 0.9 * ndl);
                let mut col = lit.mul(scene.light.color).scale(scene.light.intensity);
                // Лёгкий блик по грани.
                let h = norm(ldir.add(view));
                let spec = dot(n, h).max(0.0).powf(24.0) * 0.18;
                col = col.add(scene.light.color.scale(spec));
                best_color = col;
                shaded = true;
            }
        }
    }

    let _ = shaded;
    best_color
}

/// Отрендерить сцену в RGB-буфер.
pub fn render(scene: &Scene, width: u32, height: u32) -> Vec<u8> {
    let aspect = width as f64 / height as f64;
    let mut buf = vec![0u8; width as usize * height as usize * 3];
    let cam = &scene.camera;
    for y in 0..height {
        for x in 0..width {
            // Сглаживание 2×2 (SSAA) — дешевле, чем постобработка.
            let mut acc = Color::rgb(0.0, 0.0, 0.0);
            for sy in [0.0, 0.5] {
                for sx in [0.0, 0.5] {
                    let dir = cam.ray(
                        (x as f64 + sx + 0.25) / width as f64,
                        (y as f64 + sy + 0.25) / height as f64,
                        aspect,
                    );
                    acc = acc.add(trace(scene, cam.origin, dir, aspect));
                }
            }
            let c = acc.scale(0.25).to_rgb8();
            let i = (y as usize * width as usize + x as usize) * 3;
            buf[i] = c[0];
            buf[i + 1] = c[1];
            buf[i + 2] = c[2];
        }
    }
    let _ = det_sin(0.0);
    buf
}

// ---------------------------------------------------------------------------
// Готовые сцены
// ---------------------------------------------------------------------------

/// Сцена «корабль у планеты»: корабль крупным планом, планета и звезда в фоне.
pub fn scene_ship_and_planet() -> Scene {
    let mut s = Scene::new();
    // Планета справа-снизу — крупный фон.
    s.spheres.push(Sphere {
        center: Vec3::new(2600.0, -1500.0, 7000.0),
        radius: 2400.0,
        color: Color::rgb(0.2, 0.4, 0.6),
        terrain: true,
        emissive: 0.0,
        seed: 12345,
        atmosphere: 1.1,
    });
    // Звезда слева-сверху — источник света.
    s.spheres.push(Sphere {
        center: Vec3::new(-5200.0, 2600.0, 5000.0),
        radius: 380.0,
        color: Color::rgb(1.0, 0.93, 0.8),
        terrain: false,
        emissive: 2.6,
        seed: 1,
        atmosphere: 0.0,
    });
    s.light.direction = norm(Vec3::new(-5200.0, 2600.0, 5000.0));

    // Корабль — блочная конструкция «прямо по курсу», крупным планом.
    let hull = Color::rgb(0.74, 0.77, 0.82);
    let hull_dark = Color::rgb(0.42, 0.46, 0.55);
    let accent = Color::rgb(0.16, 0.62, 0.72);
    let glass = Color::rgb(0.12, 0.2, 0.34);
    let engine = Color::rgb(0.35, 0.95, 1.0);

    // Корпус вдоль +Z (носом от камеры), сужается к носу.
    for i in -8..=8i64 {
        let z = 1500.0 + i as f64 * 62.0;
        let w = if i.abs() > 6 { 26.0 } else if i.abs() > 3 { 46.0 } else { 58.0 };
        s.blocks.push(Block {
            center: Vec3::new(0.0, 0.0, z),
            half: Vec3::new(w, 30.0, 32.0),
            color: if i % 2 == 0 { hull } else { hull_dark },
        });
    }
    // Верхний гребень.
    for i in -4..=4i64 {
        s.blocks.push(Block {
            center: Vec3::new(0.0, 34.0, 1560.0 + i as f64 * 60.0),
            half: Vec3::new(20.0, 10.0, 26.0),
            color: hull_dark,
        });
    }
    // Крылья с акцентной полосой.
    for side in [-1.0, 1.0] {
        for i in 0..4i64 {
            s.blocks.push(Block {
                center: Vec3::new(side * (86.0 + i as f64 * 62.0), -6.0, 1440.0 + i as f64 * 36.0),
                half: Vec3::new(30.0, 9.0, 100.0),
                color: if i == 0 { accent } else { hull_dark },
            });
        }
    }
    // Кабина (тёмное стекло).
    s.blocks.push(Block {
        center: Vec3::new(0.0, 40.0, 1660.0),
        half: Vec3::new(34.0, 18.0, 70.0),
        color: glass,
    });
    // Светящиеся двигатели сзади.
    for side in [-1.0, 1.0] {
        s.blocks.push(Block {
            center: Vec3::new(side * 40.0, 0.0, 980.0),
            half: Vec3::new(24.0, 24.0, 30.0),
            color: engine,
        });
    }

    // Камера чуть сзади и выше, смотрит на корабль и планету.
    s.camera.origin = Vec3::new(-120.0, 120.0, 300.0);
    s.camera.forward = norm(Vec3::new(0.06, -0.04, 1.0));
    s.camera.fov = 1.0;
    s.star_density = 0.09;
    s
}

/// Сцена «чёрная дыра»: тёмная сфера, яркий аккреционный диск и гравитационная линза.
pub fn scene_black_hole() -> Scene {
    let mut s = Scene::new();
    // Горизонт событий — почти идеально чёрный шар.
    s.spheres.push(Sphere {
        center: Vec3::new(0.0, 0.0, 4000.0),
        radius: 520.0,
        color: Color::rgb(0.0005, 0.0005, 0.001),
        terrain: false,
        emissive: 0.0,
        seed: 7,
        atmosphere: 0.0,
    });

    // Аккреционный диск: несколько концентрических колец из блоков.
    // Внутренние — горячее (бело-голубые), внешние — холоднее (оранжевые).
    let rings = 6;
    for ring in 0..rings {
        let r0 = 640.0 + ring as f64 * 150.0;
        let per_ring = 44 + ring * 20;
        // Цвет кольца: от голубовато-белого внутри к оранжевому снаружи.
        let t = ring as f64 / (rings - 1) as f64;
        let col = if t < 0.4 {
            lerp_color(Color::rgb(1.0, 0.98, 0.92), Color::rgb(1.0, 0.85, 0.5), t / 0.4)
        } else {
            lerp_color(Color::rgb(1.0, 0.85, 0.5), Color::rgb(0.95, 0.35, 0.08), (t - 0.4) / 0.6)
        };
        for i in 0..per_ring {
            let a = i as f64 / per_ring as f64 * 6.283185307 + ring as f64 * 0.3;
            let r = r0 + (((i * 61) % 100) as f64 / 100.0 - 0.5) * 60.0;
            // Диск наклонён: z сжат, y чуть приподнят для объёма.
            let x = r * det_cos(a);
            let z = 4000.0 + r * det_sin(a) * 0.30;
            let y = (((i * 37) % 40) as f64 / 40.0 - 0.5) * 30.0;
            let h = 12.0 + ((i * 13) % 22) as f64;
            s.blocks.push(Block {
                center: Vec3::new(x, y, z),
                half: Vec3::new(20.0, h, 20.0),
                color: col,
            });
        }
    }

    s.light.direction = norm(Vec3::new(-0.2, 0.95, 0.15));
    s.light.intensity = 1.1;
    s.camera.origin = Vec3::new(0.0, 320.0, -900.0);
    s.camera.forward = norm(Vec3::new(0.0, 0.04, 1.0));
    s.camera.fov = 1.15;
    s.star_density = 0.11;
    s
}

/// Сцена «астероиды»: неровные каменные глыбы на фоне звезды.
pub fn scene_asteroids() -> Scene {
    let mut s = Scene::new();
    // Дальняя звезда — источник света.
    s.spheres.push(Sphere {
        center: Vec3::new(3000.0, 1400.0, 8000.0),
        radius: 220.0,
        color: Color::rgb(1.0, 0.95, 0.86),
        terrain: false,
        emissive: 3.0,
        seed: 3,
        atmosphere: 0.0,
    });
    s.light.direction = norm(Vec3::new(3000.0, 1400.0, 8000.0));

    // Детерминированный генератор.
    let mut h = 0x1234_5678u64;
    let mut rnd = move || {
        h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (h >> 11) as f64 / (1u64 << 53) as f64
    };

    // Каждый астероид — кластер из 3–5 блоков, поэтому силуэт неровный.
    for _ in 0..34 {
        let cx = (rnd() - 0.5) * 3400.0;
        let cy = (rnd() - 0.5) * 1600.0;
        let cz = 900.0 + rnd() * 5200.0;
        let scale = 40.0 + rnd() * 130.0;
        let base_shade = 0.30 + rnd() * 0.32;
        let base = Color::rgb(base_shade, base_shade * 0.93, base_shade * 0.82);
        let parts = 3 + (rnd() * 3.0) as i64;
        for _ in 0..parts {
            let ox = (rnd() - 0.5) * scale * 1.2;
            let oy = (rnd() - 0.5) * scale;
            let oz = (rnd() - 0.5) * scale * 1.2;
            let hx = scale * (0.35 + rnd() * 0.5);
            let hy = scale * (0.3 + rnd() * 0.5);
            let hz = scale * (0.35 + rnd() * 0.5);
            // Чуть разный оттенок у частей — камень выглядит неоднородным.
            let v = 0.8 + rnd() * 0.4;
            s.blocks.push(Block {
                center: Vec3::new(cx + ox, cy + oy, cz + oz),
                half: Vec3::new(hx, hy, hz),
                color: base.scale(v),
            });
        }
    }
    s.camera.origin = Vec3::ZERO;
    s.camera.forward = Vec3::new(0.02, 0.0, 1.0);
    s.camera.fov = 1.25;
    s.star_density = 0.1;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_sphere_in_front() {
        let s = Sphere {
            center: Vec3::new(0.0, 0.0, 100.0),
            radius: 10.0,
            color: Color::rgb(1.0, 1.0, 1.0),
            terrain: false,
            emissive: 0.0,
            seed: 0,
            atmosphere: 0.0,
        };
        let hit = s.hit(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        assert!(hit.is_some());
        let (t, _, _) = hit.unwrap();
        assert!((t - 90.0).abs() < 1e-6);
    }

    #[test]
    fn ray_misses_sphere_behind() {
        let s = Sphere {
            center: Vec3::new(0.0, 0.0, -100.0),
            radius: 10.0,
            color: Color::rgb(1.0, 1.0, 1.0),
            terrain: false,
            emissive: 0.0,
            seed: 0,
            atmosphere: 0.0,
        };
        assert!(s.hit(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0)).is_none());
    }

    #[test]
    fn ray_hits_block() {
        let b = Block {
            center: Vec3::new(0.0, 0.0, 50.0),
            half: Vec3::new(5.0, 5.0, 5.0),
            color: Color::rgb(1.0, 1.0, 1.0),
        };
        assert!(b.hit(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0)).is_some());
        assert!(b.hit(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn render_produces_correct_buffer_size() {
        let s = scene_ship_and_planet();
        let buf = render(&s, 64, 48);
        assert_eq!(buf.len(), 64 * 48 * 3);
    }

    #[test]
    fn render_is_deterministic() {
        let s = scene_asteroids();
        let a = render(&s, 32, 32);
        let b = render(&s, 32, 32);
        assert_eq!(a, b);
    }

    #[test]
    fn emissive_star_is_bright() {
        let s = scene_ship_and_planet();
        // Берём координаты звезды прямо из сцены (они меняются вместе с ней).
        let star = s
            .spheres
            .iter()
            .find(|sp| sp.emissive > 0.0)
            .expect("в сцене должна быть звезда");
        let dir = norm(star.center);
        let c = trace(&s, Vec3::ZERO, dir, 1.0);
        let lum = c.r + c.g + c.b;
        assert!(lum > 0.5, "звезда должна быть яркой, получено {lum}");
    }

    #[test]
    fn background_has_stars_or_empty_space() {
        let s = Scene::new();
        // Смотрим в «пустое» направление.
        let c = trace(&s, Vec3::ZERO, Vec3::new(0.1, 0.2, 0.97), 1.0);
        // Космос тёмный, но не обязан быть абсолютно нулевым.
        assert!(c.r < 1.0 && c.g < 1.0 && c.b < 1.0);
    }

    #[test]
    fn noise_is_in_unit_range() {
        for i in 0..200 {
            let p = Vec3::new(i as f64 * 3.1, i as f64 * 1.7, i as f64 * 0.9);
            let n = fbm(p, 42);
            assert!((0.0..=1.0).contains(&n), "шум вне диапазона: {n}");
        }
    }

    #[test]
    fn png_encoder_writes_file() {
        let dir = std::env::temp_dir().join("ou-render-test.png");
        let buf = vec![128u8; 16 * 16 * 3];
        png::write_png(dir.to_str().unwrap(), 16, 16, &buf).unwrap();
        let meta = std::fs::metadata(&dir).unwrap();
        assert!(meta.len() > 8);
        let _ = std::fs::remove_file(&dir);
    }
}
