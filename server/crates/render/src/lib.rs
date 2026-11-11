//! # ou-render
//!
//! РЎРѕС„С‚РІРµСЂРЅС‹Р№ СЂРµРЅРґРµСЂРµСЂ Open Universe: РЅР°СЃС‚РѕСЏС‰Р°СЏ С‚СЂР°СЃСЃРёСЂРѕРІРєР° Р»СѓС‡РµР№ РЅР° CPU,
//! Р±РµР· GPU Рё Р±РµР· РІРЅРµС€РЅРёС… Р±РёР±Р»РёРѕС‚РµРє.
//!
//! ## Р—Р°С‡РµРј
//! РќР°С€ РєР»РёРµРЅС‚ РїРѕ ADR-0001 вЂ” Unreal Engine 5. РќРѕ РїРѕРєР° UE РЅРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ, Р° РІСЃСЏ
//! *Р»РѕРіРёРєР°* РјРёСЂР° (РїРѕР·РёС†РёРё, РѕСЂРёРµРЅС‚Р°С†РёРё, СЃРЅР°РїС€РѕС‚С‹) СѓР¶Рµ РµСЃС‚СЊ РЅР° СЃРµСЂРІРµСЂРµ, РЅСѓР¶РµРЅ
//! СЃРїРѕСЃРѕР± **СѓРІРёРґРµС‚СЊ** СЌС‚РѕС‚ РјРёСЂ. Р­С‚РѕС‚ РєСЂРµР№С‚ СЂРµРЅРґРµСЂРёС‚ СЃС†РµРЅСѓ РёР· С‚РѕРіРѕ Р¶Рµ СЃРѕСЃС‚РѕСЏРЅРёСЏ,
//! С‡С‚Рѕ РїСЂРёС…РѕРґРёС‚ РїРѕ СЃРµС‚Рё, Рё СЃРѕС…СЂР°РЅСЏРµС‚ РєР°РґСЂ РІ PNG.
//!
//! РљРѕРјРїРѕРЅРµРЅС‚С‹ (РєР°РјРµСЂР°, floating origin, СЃС„РµСЂР°-РїР»Р°РЅРµС‚Р°, Р±Р»РѕС‡РЅС‹Р№ РєРѕСЂР°Р±Р»СЊ, Р·РІС‘Р·РґС‹)
//! РїРµСЂРµРЅРѕСЃСЏС‚СЃСЏ РІ UE5 РѕРґРёРЅ-Рє-РѕРґРЅРѕРјСѓ вЂ” РјР°С‚РµРјР°С‚РёРєР° Р·РґРµСЃСЊ С‚Р° Р¶Рµ, С‡С‚Рѕ РїРѕРЅР°РґРѕР±РёС‚СЃСЏ С‚Р°Рј.
//!
//! ## Р§С‚Рѕ СЂРµРЅРґРµСЂРёС‚СЃСЏ
//! * Р·РІС‘Р·РґРЅРѕРµ РЅРµР±Рѕ (РґРµС‚РµСЂРјРёРЅРёСЂРѕРІР°РЅРЅРѕРµ, РёР· С…РµС€Р° РЅР°РїСЂР°РІР»РµРЅРёСЏ),
//! * РїР»Р°РЅРµС‚Р°-СЃС„РµСЂР° СЃ РїСЂРѕС†РµРґСѓСЂРЅС‹Рј С€СѓРјРѕРј Рё РѕСЃРІРµС‰РµРЅРёРµРј РїРѕ Р›Р°РјР±РµСЂС‚Сѓ,
//! * РєРѕСЂР°Р±Р»СЊ РєР°Рє РЅР°Р±РѕСЂ Р±Р»РѕРєРѕРІ (OBB) вЂ” РІ РґСѓС…Рµ РІРѕРєСЃРµР»СЊРЅС‹С… РєРѕРЅСЃС‚СЂСѓРєС†РёР№,
//! * Р°С‚РјРѕСЃС„РµСЂРЅРѕРµ СЃРІРµС‡РµРЅРёРµ РїРѕ РєСЂР°СЋ РїР»Р°РЅРµС‚С‹,
//! * РёСЃС‚РѕС‡РЅРёРє СЃРІРµС‚Р° (Р·РІРµР·РґР°) СЃ Р±Р»РёРєРѕРј.
//!
//! ## РџСЂРѕРёР·РІРѕРґРёС‚РµР»СЊРЅРѕСЃС‚СЊ
//! Р­С‚Рѕ СѓС‡РµР±РЅС‹Р№/РѕС‚Р»Р°РґРѕС‡РЅС‹Р№ СЂРµРЅРґРµСЂ: РѕРЅ С‡РµСЃС‚РЅРѕ С‚СЂР°СЃСЃРёСЂСѓРµС‚ Р»СѓС‡Рё, РЅРѕ РЅРµ РіРѕРЅСЏРµС‚СЃСЏ Р·Р°
//! РєР°РґСЂР°РјРё РІ СЃРµРєСѓРЅРґСѓ. РћРЅ РґР°С‘С‚ **СЂРµС„РµСЂРµРЅСЃРЅС‹Р№ РєР°РґСЂ** РґР»СЏ СЃРІРµСЂРєРё Рё СЃРєСЂРёРЅС€РѕС‚.

pub mod png;
pub mod voxel;

use ou_protocol::Vec3;
use ou_sim::{det_cos, det_sin};

// ---------------------------------------------------------------------------
// РњР°С‚РµРјР°С‚РёРєР°
// ---------------------------------------------------------------------------

/// РќРѕСЂРјР°Р»РёР·РѕРІР°С‚СЊ РІРµРєС‚РѕСЂ.
pub(crate) fn norm(v: Vec3) -> Vec3 {
    let l = v.length();
    if l == 0.0 {
        Vec3::ZERO
    } else {
        v.scale(1.0 / l)
    }
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// РЎРјРµС€Р°С‚СЊ РґРІР° РІРµРєС‚РѕСЂР°.
#[allow(dead_code)]
fn lerp3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    a.add(b.sub(a).scale(t))
}

/// РЎРјРµС€Р°С‚СЊ РґРІР° С†РІРµС‚Р°.
pub(crate) fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    a.add(b.add(a.scale(-1.0)).scale(t))
}

/// РҐРµС€ СЂРµС€С‘С‚РєРё РІ [0,1).
pub(crate) fn lattice_hash(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed;
    for c in [x, y, z] {
        h ^= (c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// РџР»Р°РІРЅР°СЏ РёРЅС‚РµСЂРїРѕР»СЏС†РёСЏ (smoothstep).
pub(crate) fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// РўСЂРёР»РёРЅРµР№РЅС‹Р№ value noise: СѓР·Р»С‹ СЂРµС€С‘С‚РєРё СЃРіР»Р°Р¶РёРІР°СЋС‚СЃСЏ, РїРѕСЌС‚РѕРјСѓ РЅРµС‚ В«РєР»РµС‚РѕРєВ».
pub(crate) fn value_noise(p: Vec3, freq: f64, seed: u64) -> f64 {
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

/// Р¤СЂР°РєС‚Р°Р»СЊРЅС‹Р№ С€СѓРј (РЅРµСЃРєРѕР»СЊРєРѕ РѕРєС‚Р°РІ) вЂ” РїРѕС…РѕР¶Рµ РЅР° РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ РїР»Р°РЅРµС‚С‹.
pub(crate) fn fbm(p: Vec3, seed: u64) -> f64 {
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
// РЎС†РµРЅР°
// ---------------------------------------------------------------------------

/// Р¦РІРµС‚ РІ Р»РёРЅРµР№РЅРѕРј РїСЂРѕСЃС‚СЂР°РЅСЃС‚РІРµ 0..1.
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
    pub fn scale(self, k: f64) -> Self {
        Color::rgb(self.r * k, self.g * k, self.b * k)
    }
    pub fn add(self, o: Color) -> Self {
        Color::rgb(self.r + o.r, self.g + o.g, self.b + o.b)
    }
    #[allow(dead_code)]
    pub fn mul(self, o: Color) -> Self {
        Color::rgb(self.r * o.r, self.g * o.g, self.b * o.b)
    }
    pub fn to_rgb8(self) -> [u8; 3] {
        // Р“Р°РјРјР°-РєРѕСЂСЂРµРєС†РёСЏ Рё РѕРіСЂР°РЅРёС‡РµРЅРёРµ.
        let g = |v: f64| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
        [g(self.r), g(self.g), g(self.b)]
    }
}

/// РњР°С‚СЂРёС†Р° РїРѕРІРѕСЂРѕС‚Р° 3Г—3 (СЃС‚РѕР»Р±С†С‹ вЂ” Р»РѕРєР°Р»СЊРЅС‹Рµ РѕСЃРё РІ РјРёСЂРѕРІС‹С… РєРѕРѕСЂРґРёРЅР°С‚Р°С…).
#[derive(Clone, Copy, Debug)]
pub struct Mat3 {
    pub c0: Vec3,
    pub c1: Vec3,
    pub c2: Vec3,
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 {
        c0: Vec3::new(1.0, 0.0, 0.0),
        c1: Vec3::new(0.0, 1.0, 0.0),
        c2: Vec3::new(0.0, 0.0, 1.0),
    };

    /// РџРѕРІРѕСЂРѕС‚ РёР· СѓРіР»РѕРІ СЂС‹СЃРєР°РЅРёСЏ Рё С‚Р°РЅРіР°Р¶Р° (РєР°Рє РІ СЃРёРјСѓР»СЏС†РёРё РєРѕСЂР°Р±Р»СЏ).
    pub fn from_yaw_pitch(yaw: f64, pitch: f64) -> Self {
        let (sy, cy) = (det_sin(yaw), det_cos(yaw));
        let (sp, cp) = (det_sin(pitch), det_cos(pitch));
        // Р‘Р°Р·РёСЃ: X вЂ” РІРїСЂР°РІРѕ, Y вЂ” РІРІРµСЂС… (СЃ СѓС‡С‘С‚РѕРј С‚Р°РЅРіР°Р¶Р°), Z вЂ” В«РІРїРµСЂС‘РґВ».
        Mat3 {
            c0: Vec3::new(cy, 0.0, -sy),
            c1: Vec3::new(-sy * sp, cp, -cy * sp),
            c2: Vec3::new(sy * cp, sp, cy * cp),
        }
    }

    /// РЈРјРЅРѕР¶РёС‚СЊ РјР°С‚СЂРёС†Сѓ РЅР° РІРµРєС‚РѕСЂ (MВ·v).
    pub fn mul_vec(self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.c0.x * v.x + self.c1.x * v.y + self.c2.x * v.z,
            self.c0.y * v.x + self.c1.y * v.y + self.c2.y * v.z,
            self.c0.z * v.x + self.c1.z * v.y + self.c2.z * v.z,
        )
    }

    /// РўСЂР°РЅСЃРїРѕРЅРёСЂРѕРІР°РЅРёРµ (РѕР±СЂР°С‚РЅР°СЏ РґР»СЏ РѕСЂС‚РѕРЅРѕСЂРјР°Р»СЊРЅРѕР№ РјР°С‚СЂРёС†С‹).
    pub fn transposed(self) -> Mat3 {
        Mat3 {
            c0: Vec3::new(self.c0.x, self.c1.x, self.c2.x),
            c1: Vec3::new(self.c0.y, self.c1.y, self.c2.y),
            c2: Vec3::new(self.c0.z, self.c1.z, self.c2.z),
        }
    }
}

/// РћСЂРёРµРЅС‚РёСЂРѕРІР°РЅРЅС‹Р№ Р±Р»РѕРє (РґРµС‚Р°Р»СЊ РєРѕСЂР°Р±Р»СЏ).
#[derive(Clone, Debug)]
pub struct Block {
    pub center: Vec3,
    pub half: Vec3,
    /// Р‘Р°Р·РѕРІС‹Р№ С†РІРµС‚.
    pub color: Color,
    /// РћСЂРёРµРЅС‚Р°С†РёСЏ РІ РјРёСЂРµ (РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ вЂ” Р±РµР· РїРѕРІРѕСЂРѕС‚Р°).
    pub rot: Mat3,
}

impl Block {
    /// Р‘Р»РѕРє Р±РµР· РїРѕРІРѕСЂРѕС‚Р° (РѕСЃРё РІС‹СЂРѕРІРЅРµРЅС‹ РїРѕ РјРёСЂРѕРІС‹Рј).
    pub fn axis(center: Vec3, half: Vec3, color: Color) -> Self {
        Block { center, half, color, rot: Mat3::IDENTITY }
    }

    /// Р‘Р»РѕРє СЃ РѕСЂРёРµРЅС‚Р°С†РёРµР№.
    pub fn oriented(center: Vec3, half: Vec3, color: Color, rot: Mat3) -> Self {
        Block { center, half, color, rot }
    }

    /// РџРµСЂРµСЃРµС‡РµРЅРёРµ Р»СѓС‡Р° СЃ OBB. Р’РѕР·РІСЂР°С‰Р°РµС‚ СЂР°СЃСЃС‚РѕСЏРЅРёРµ Рё РјРёСЂРѕРІСѓСЋ РЅРѕСЂРјР°Р»СЊ.
    ///
    /// Р›СѓС‡ РїРµСЂРµРІРѕРґРёС‚СЃСЏ РІ Р»РѕРєР°Р»СЊРЅС‹Рµ РєРѕРѕСЂРґРёРЅР°С‚С‹ Р±Р»РѕРєР° (С‚Р°Рј РѕРЅ СЃРЅРѕРІР° AABB),
    /// РїРѕСЌС‚РѕРјСѓ РїРѕРІРѕСЂРѕС‚ РЅРµ РІР»РёСЏРµС‚ РЅР° СЃРєРѕСЂРѕСЃС‚СЊ РїСЂРѕРІРµСЂРєРё.
    fn hit_ex(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3)> {
        let inv = self.rot.transposed();
        let lo = inv.mul_vec(origin.sub(self.center));
        let ld = inv.mul_vec(dir);

        let mut tmin = f64::NEG_INFINITY;
        let mut tmax = f64::INFINITY;
        let mut normal_axis = 0usize;
        let mut normal_sign = 1.0;
        let axes = [
            (lo.x, ld.x, self.half.x),
            (lo.y, ld.y, self.half.y),
            (lo.z, ld.z, self.half.z),
        ];
        for (i, (o, d, h)) in axes.iter().enumerate() {
            if d.abs() < 1e-9 {
                if o.abs() > *h {
                    return None;
                }
            } else {
                let t1 = (-h - o) / d;
                let t2 = (h - o) / d;
                let (near, far, sign) = if t1 < t2 {
                    (t1, t2, -1.0)
                } else {
                    (t2, t1, 1.0)
                };
                if near > tmin {
                    tmin = near;
                    normal_axis = i;
                    normal_sign = sign;
                }
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
        let local_n = match normal_axis {
            0 => Vec3::new(normal_sign, 0.0, 0.0),
            1 => Vec3::new(0.0, normal_sign, 0.0),
            _ => Vec3::new(0.0, 0.0, normal_sign),
        };
        Some((t, norm(self.rot.mul_vec(local_n))))
    }

    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3)> {
        self.hit_ex(origin, dir)
    }
}

/// РЎС„РµСЂР° (РїР»Р°РЅРµС‚Р° РёР»Рё Р·РІРµР·РґР°).
#[derive(Clone, Debug)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f64,
    /// Р¦РІРµС‚ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё.
    pub color: Color,
    /// Р•СЃС‚СЊ Р»Рё РїСЂРѕС†РµРґСѓСЂРЅС‹Р№ СЂРµР»СЊРµС„ (РїР»Р°РЅРµС‚Р°) РёР»Рё СЂРѕРІРЅС‹Р№ С†РІРµС‚ (Р·РІРµР·РґР°).
    pub terrain: bool,
    /// РЎРѕР±СЃС‚РІРµРЅРЅРѕРµ СЃРІРµС‡РµРЅРёРµ (РґР»СЏ Р·РІРµР·РґС‹).
    pub emissive: f64,
    pub seed: u64,
    /// РўРѕР»С‰РёРЅР° Р°С‚РјРѕСЃС„РµСЂС‹ (0 вЂ” РЅРµС‚).
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

/// РљР°РјРµСЂР°.
#[derive(Clone, Debug)]
pub struct Camera {
    pub origin: Vec3,
    /// РљСѓРґР° СЃРјРѕС‚СЂРёС‚.
    pub forward: Vec3,
    pub up: Vec3,
    /// РџРѕР»Рµ Р·СЂРµРЅРёСЏ РїРѕ РІРµСЂС‚РёРєР°Р»Рё (СЂР°РґРёР°РЅС‹).
    pub fov: f64,
}

impl Camera {
    /// Р›СѓС‡ РґР»СЏ РїРёРєСЃРµР»СЏ (nx, ny в€€ [0,1]).
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

/// РћСЃРІРµС‰РµРЅРёРµ.
#[derive(Clone, Debug)]
pub struct Light {
    pub direction: Vec3,
    pub color: Color,
    /// РќР°РїСЂР°РІР»РµРЅРёРµ, РєСѓРґР° СЃРІРµС‚РёС‚ (РЅРѕСЂРјР°Р»РёР·РѕРІР°РЅРѕ Рє РёСЃС‚РѕС‡РЅРёРєСѓ).
    pub intensity: f64,
}

/// Р¦РµР»РёРєРѕРј СЃС†РµРЅР°.
#[derive(Clone, Debug)]
pub struct Scene {
    pub camera: Camera,
    pub spheres: Vec<Sphere>,
    pub blocks: Vec<Block>,
    pub light: Light,
    /// Р¦РІРµС‚ РіР»СѓР±РѕРєРѕРіРѕ РєРѕСЃРјРѕСЃР°.
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
// РўСЂР°СЃСЃРёСЂРѕРІРєР°
// ---------------------------------------------------------------------------

/// Р¤РѕРЅ: Р·РІС‘Р·РґС‹ С‚РѕС‡РєР°РјРё, С‚СѓРјР°РЅРЅРѕСЃС‚СЊ Рё В«РіР°Р»Р°РєС‚РёС‡РµСЃРєР°СЏ РїРѕР»РѕСЃР°В».
fn background(dir: Vec3, scene: &Scene) -> Color {
    let d = norm(dir);
    let mut col = scene.space;

    // Р—РІС‘Р·РґС‹: СЂРµС€С‘С‚РєР° РЅР°РїСЂР°РІР»РµРЅРёР№; РІ СЂРµРґРєРёС… СЏС‡РµР№РєР°С… СЃС‚Р°РІРёРј С‚РѕС‡РєСѓ Рё СЂРёСЃСѓРµРј
    // РµС‘ РєР°Рє РјР°Р»РµРЅСЊРєРёР№ РіР°СѓСЃСЃРѕРІ РѕСЂРµРѕР», РїРѕСЌС‚РѕРјСѓ Р·РІС‘Р·РґС‹ РєСЂСѓРіР»С‹Рµ, Р° РЅРµ РєРІР°РґСЂР°С‚РЅС‹Рµ.
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
                // РЎРјРµС‰РµРЅРёРµ Р·РІРµР·РґС‹ РІРЅСѓС‚СЂРё СЏС‡РµР№РєРё.
                let ox = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x11);
                let oy = lattice_hash(cx, cy, cz, scene.star_seed ^ 0x22);
                // РџРѕР»РѕР¶РµРЅРёРµ Р·РІРµР·РґС‹ РІ РїСЂРѕСЃС‚СЂР°РЅСЃС‚РІРµ СЏС‡РµРµРє.
                let sx = cx as f64 + ox - xf;
                let sy = cy as f64 + oy - yf;
                let sz = cz as f64 - zf;
                let dist2 = sx * sx + sy * sy + sz * sz;
                // РћСЂРµРѕР»: С‡РµРј Р±Р»РёР¶Рµ Рє С‚РѕС‡РєРµ, С‚РµРј СЏСЂС‡Рµ.
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

    // РўСѓРјР°РЅРЅРѕСЃС‚СЊ: РЅРёР·РєРѕС‡Р°СЃС‚РѕС‚РЅС‹Р№ С€СѓРј РїРѕ РЅР°РїСЂР°РІР»РµРЅРёСЋ.
    let n = fbm(Vec3::new(d.x * 2.2, d.y * 2.2, d.z * 2.2), scene.star_seed ^ 0xABCD);
    if n > 0.55 {
        let t = ((n - 0.55) / 0.45).min(1.0);
        let neb = Color::rgb(0.10, 0.05, 0.24).add(Color::rgb(0.06, 0.03, 0.14).scale(t));
        col = lerp_color(col, neb, t * 0.6);
    }

    // Р“Р°Р»Р°РєС‚РёС‡РµСЃРєР°СЏ РїРѕР»РѕСЃР° вЂ” РјСЏРіРєР°СЏ СЃРІРµС‚Р»Р°СЏ РїРѕР»РѕСЃР° РїРѕРїРµСЂС‘Рє РЅРµР±Р°.
    let band = (d.y * 3.2).abs();
    if band < 0.5 {
        let t = 1.0 - band / 0.5;
        col = col.add(Color::rgb(0.018, 0.02, 0.035).scale(t * t));
    }
    col
}

/// РћСЃРІРµС‚РёС‚СЊ С‚РѕС‡РєСѓ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё: ambient + diffuse + Р±Р»РёРє.
fn shade(base: Color, n: Vec3, view: Vec3, ldir: Vec3, light: &Light, specular: f64) -> Color {
    let ndl = dot(n, ldir).max(0.0);
    let ambient = 0.06;
    let mut col = base.mul(light.color).scale((ambient + (1.0 - ambient) * ndl) * light.intensity);

    // Р‘Р»РёРє (Р‘Р»РёРЅРЅ-Р¤РѕРЅРі).
    if specular > 0.0 && ndl > 0.0 {
        let h = norm(ldir.add(view));
        let ndh = dot(n, h).max(0.0);
        let spec = ndh.powf(48.0) * specular;
        col = col.add(light.color.scale(spec * light.intensity));
    }
    col
}

/// РўСЂР°СЃСЃРёСЂРѕРІР°С‚СЊ РѕРґРёРЅ Р»СѓС‡ Рё РІРµСЂРЅСѓС‚СЊ С†РІРµС‚.
pub fn trace(scene: &Scene, origin: Vec3, dir: Vec3, aspect: f64) -> Color {
    let _ = aspect;
    let ldir = norm(scene.light.direction);
    let view = norm(dir.scale(-1.0));

    // Р‘Р»РёР¶Р°Р№С€РµРµ РїРµСЂРµСЃРµС‡РµРЅРёРµ.
    let mut best_t = f64::INFINITY;
    let mut best_color = background(dir, scene);
    let mut shaded = false;

    for s in &scene.spheres {
        if let Some((t, _p, rel)) = s.hit(origin, dir) {
            if t < best_t {
                best_t = t;
                if s.emissive > 0.0 {
                    // РЎРѕР±СЃС‚РІРµРЅРЅРѕРµ СЃРІРµС‡РµРЅРёРµ (Р·РІРµР·РґР°).
                    best_color = s.color.scale(s.emissive);
                    shaded = true;
                } else {
                    let n = norm(rel);
                    // Р РµР»СЊРµС„: СЃСѓС€Р°/РѕРєРµР°РЅ РїРѕ РїСЂРѕС†РµРґСѓСЂРЅРѕРјСѓ С€СѓРјСѓ.
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
                    // РђС‚РјРѕСЃС„РµСЂРЅРѕРµ СЃРІРµС‡РµРЅРёРµ РїРѕ РєСЂР°СЋ (С„СЂРµРЅРµР»СЊ).
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

    // Р‘Р»РѕРєРё (РґРµС‚Р°Р»Рё РєРѕСЂР°Р±Р»СЏ, Р°СЃС‚РµСЂРѕРёРґС‹).
    for b in &scene.blocks {
        if let Some((t, n)) = b.hit_ex(origin, dir) {
            if t < best_t {
                best_t = t;
                let p = origin.add(dir.scale(t));
                // РљР°Р¶РґС‹Р№ Р±Р»РѕРє С‡СѓС‚СЊ РѕС‚Р»РёС‡Р°РµС‚СЃСЏ РѕС‚С‚РµРЅРєРѕРј вЂ” В«РІРѕРєСЃРµР»СЊРЅР°СЏВ» С‚РµРєСЃС‚СѓСЂР°.
                let tnoise = 0.9 + 0.25 * value_noise(p, 6.0, 0x7777);
                // РЈ Р±Р»РѕС‡РЅС‹С… РїРѕРІРµСЂС…РЅРѕСЃС‚РµР№ СЃР»Р°Р±С‹Р№ ambient, РёРЅР°С‡Рµ РєР°РјРЅРё В«С‚РѕРЅСѓС‚В».
                let ndl = dot(n, ldir).max(0.0);
                let lit = b.color.scale(tnoise).scale(0.32 + 0.9 * ndl);
                let mut col = lit.mul(scene.light.color).scale(scene.light.intensity);
                // Р›С‘РіРєРёР№ Р±Р»РёРє РїРѕ РіСЂР°РЅРё.
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

/// РћС‚СЂРµРЅРґРµСЂРёС‚СЊ СЃС†РµРЅСѓ РІ RGB-Р±СѓС„РµСЂ.
pub fn render(scene: &Scene, width: u32, height: u32) -> Vec<u8> {
    let aspect = width as f64 / height as f64;
    let mut buf = vec![0u8; width as usize * height as usize * 3];
    let cam = &scene.camera;
    for y in 0..height {
        for x in 0..width {
            // РЎРіР»Р°Р¶РёРІР°РЅРёРµ 2Г—2 (SSAA) вЂ” РґРµС€РµРІР»Рµ, С‡РµРј РїРѕСЃС‚РѕР±СЂР°Р±РѕС‚РєР°.
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
// Р“РѕС‚РѕРІС‹Рµ СЃС†РµРЅС‹
// ---------------------------------------------------------------------------

/// РЎС†РµРЅР° В«РєРѕСЂР°Р±Р»СЊ Сѓ РїР»Р°РЅРµС‚С‹В»: РєРѕСЂР°Р±Р»СЊ РєСЂСѓРїРЅС‹Рј РїР»Р°РЅРѕРј, РїР»Р°РЅРµС‚Р° Рё Р·РІРµР·РґР° РІ С„РѕРЅРµ.
pub fn scene_ship_and_planet() -> Scene {
    let mut s = Scene::new();
    // РџР»Р°РЅРµС‚Р° СЃРїСЂР°РІР°-СЃРЅРёР·Сѓ вЂ” РєСЂСѓРїРЅС‹Р№ С„РѕРЅ.
    s.spheres.push(Sphere {
        center: Vec3::new(2600.0, -1500.0, 7000.0),
        radius: 2400.0,
        color: Color::rgb(0.2, 0.4, 0.6),
        terrain: true,
        emissive: 0.0,
        seed: 12345,
        atmosphere: 1.1,
    });
    // Р—РІРµР·РґР° СЃР»РµРІР°-СЃРІРµСЂС…Сѓ вЂ” РёСЃС‚РѕС‡РЅРёРє СЃРІРµС‚Р°.
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

    // РљРѕСЂР°Р±Р»СЊ вЂ” Р±Р»РѕС‡РЅР°СЏ РєРѕРЅСЃС‚СЂСѓРєС†РёСЏ В«РїСЂСЏРјРѕ РїРѕ РєСѓСЂСЃСѓВ», РєСЂСѓРїРЅС‹Рј РїР»Р°РЅРѕРј.
    let hull = Color::rgb(0.74, 0.77, 0.82);
    let hull_dark = Color::rgb(0.42, 0.46, 0.55);
    let accent = Color::rgb(0.16, 0.62, 0.72);
    let glass = Color::rgb(0.12, 0.2, 0.34);
    let engine = Color::rgb(0.35, 0.95, 1.0);

    // РљРѕСЂРїСѓСЃ РІРґРѕР»СЊ +Z (РЅРѕСЃРѕРј РѕС‚ РєР°РјРµСЂС‹), СЃСѓР¶Р°РµС‚СЃСЏ Рє РЅРѕСЃСѓ.
    for i in -8..=8i64 {
        let z = 1500.0 + i as f64 * 62.0;
        let w = if i.abs() > 6 { 26.0 } else if i.abs() > 3 { 46.0 } else { 58.0 };
        s.blocks.push(Block::axis(Vec3::new(0.0, 0.0, z), Vec3::new(w, 30.0, 32.0), if i % 2 == 0 { hull } else { hull_dark }));
    }
    // Р’РµСЂС…РЅРёР№ РіСЂРµР±РµРЅСЊ.
    for i in -4..=4i64 {
        s.blocks.push(Block::axis(Vec3::new(0.0, 34.0, 1560.0 + i as f64 * 60.0), Vec3::new(20.0, 10.0, 26.0), hull_dark));
    }
    // РљСЂС‹Р»СЊСЏ СЃ Р°РєС†РµРЅС‚РЅРѕР№ РїРѕР»РѕСЃРѕР№.
    for side in [-1.0, 1.0] {
        for i in 0..4i64 {
            s.blocks.push(Block::axis(Vec3::new(side * (86.0 + i as f64 * 62.0), -6.0, 1440.0 + i as f64 * 36.0), Vec3::new(30.0, 9.0, 100.0), if i == 0 { accent } else { hull_dark }));
        }
    }
    // РљР°Р±РёРЅР° (С‚С‘РјРЅРѕРµ СЃС‚РµРєР»Рѕ).
    s.blocks.push(Block::axis(Vec3::new(0.0, 40.0, 1660.0), Vec3::new(34.0, 18.0, 70.0), glass));
    // РЎРІРµС‚СЏС‰РёРµСЃСЏ РґРІРёРіР°С‚РµР»Рё СЃР·Р°РґРё.
    for side in [-1.0, 1.0] {
        s.blocks.push(Block::axis(Vec3::new(side * 40.0, 0.0, 980.0), Vec3::new(24.0, 24.0, 30.0), engine));
    }

    // РљР°РјРµСЂР° С‡СѓС‚СЊ СЃР·Р°РґРё Рё РІС‹С€Рµ, СЃРјРѕС‚СЂРёС‚ РЅР° РєРѕСЂР°Р±Р»СЊ Рё РїР»Р°РЅРµС‚Сѓ.
    s.camera.origin = Vec3::new(-120.0, 120.0, 300.0);
    s.camera.forward = norm(Vec3::new(0.06, -0.04, 1.0));
    s.camera.fov = 1.0;
    s.star_density = 0.09;
    s
}

/// РЎС†РµРЅР° В«С‡С‘СЂРЅР°СЏ РґС‹СЂР°В»: С‚С‘РјРЅР°СЏ СЃС„РµСЂР°, СЏСЂРєРёР№ Р°РєРєСЂРµС†РёРѕРЅРЅС‹Р№ РґРёСЃРє Рё РіСЂР°РІРёС‚Р°С†РёРѕРЅРЅР°СЏ Р»РёРЅР·Р°.
pub fn scene_black_hole() -> Scene {
    let mut s = Scene::new();
    // Р“РѕСЂРёР·РѕРЅС‚ СЃРѕР±С‹С‚РёР№ вЂ” РїРѕС‡С‚Рё РёРґРµР°Р»СЊРЅРѕ С‡С‘СЂРЅС‹Р№ С€Р°СЂ.
    s.spheres.push(Sphere {
        center: Vec3::new(0.0, 0.0, 4000.0),
        radius: 520.0,
        color: Color::rgb(0.0005, 0.0005, 0.001),
        terrain: false,
        emissive: 0.0,
        seed: 7,
        atmosphere: 0.0,
    });

    // РђРєРєСЂРµС†РёРѕРЅРЅС‹Р№ РґРёСЃРє: РЅРµСЃРєРѕР»СЊРєРѕ РєРѕРЅС†РµРЅС‚СЂРёС‡РµСЃРєРёС… РєРѕР»РµС† РёР· Р±Р»РѕРєРѕРІ.
    // Р’РЅСѓС‚СЂРµРЅРЅРёРµ вЂ” РіРѕСЂСЏС‡РµРµ (Р±РµР»Рѕ-РіРѕР»СѓР±С‹Рµ), РІРЅРµС€РЅРёРµ вЂ” С…РѕР»РѕРґРЅРµРµ (РѕСЂР°РЅР¶РµРІС‹Рµ).
    let rings = 6;
    for ring in 0..rings {
        let r0 = 640.0 + ring as f64 * 150.0;
        let per_ring = 44 + ring * 20;
        // Р¦РІРµС‚ РєРѕР»СЊС†Р°: РѕС‚ РіРѕР»СѓР±РѕРІР°С‚Рѕ-Р±РµР»РѕРіРѕ РІРЅСѓС‚СЂРё Рє РѕСЂР°РЅР¶РµРІРѕРјСѓ СЃРЅР°СЂСѓР¶Рё.
        let t = ring as f64 / (rings - 1) as f64;
        let col = if t < 0.4 {
            lerp_color(Color::rgb(1.0, 0.98, 0.92), Color::rgb(1.0, 0.85, 0.5), t / 0.4)
        } else {
            lerp_color(Color::rgb(1.0, 0.85, 0.5), Color::rgb(0.95, 0.35, 0.08), (t - 0.4) / 0.6)
        };
        for i in 0..per_ring {
            let a = i as f64 / per_ring as f64 * 6.283185307 + ring as f64 * 0.3;
            let r = r0 + (((i * 61) % 100) as f64 / 100.0 - 0.5) * 60.0;
            // Р”РёСЃРє РЅР°РєР»РѕРЅС‘РЅ: z СЃР¶Р°С‚, y С‡СѓС‚СЊ РїСЂРёРїРѕРґРЅСЏС‚ РґР»СЏ РѕР±СЉС‘РјР°.
            let x = r * det_cos(a);
            let z = 4000.0 + r * det_sin(a) * 0.30;
            let y = (((i * 37) % 40) as f64 / 40.0 - 0.5) * 30.0;
            let h = 12.0 + ((i * 13) % 22) as f64;
            s.blocks.push(Block::axis(Vec3::new(x, y, z), Vec3::new(20.0, h, 20.0), col));
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

/// РЎС†РµРЅР° В«Р°СЃС‚РµСЂРѕРёРґС‹В»: РЅРµСЂРѕРІРЅС‹Рµ РєР°РјРµРЅРЅС‹Рµ РіР»С‹Р±С‹ РЅР° С„РѕРЅРµ Р·РІРµР·РґС‹.
pub fn scene_asteroids() -> Scene {
    let mut s = Scene::new();
    // Р”Р°Р»СЊРЅСЏСЏ Р·РІРµР·РґР° вЂ” РёСЃС‚РѕС‡РЅРёРє СЃРІРµС‚Р°.
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

    // Р”РµС‚РµСЂРјРёРЅРёСЂРѕРІР°РЅРЅС‹Р№ РіРµРЅРµСЂР°С‚РѕСЂ.
    let mut h = 0x1234_5678u64;
    let mut rnd = move || {
        h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (h >> 11) as f64 / (1u64 << 53) as f64
    };

    // РљР°Р¶РґС‹Р№ Р°СЃС‚РµСЂРѕРёРґ вЂ” РєР»Р°СЃС‚РµСЂ РёР· 3вЂ“5 Р±Р»РѕРєРѕРІ, РїРѕСЌС‚РѕРјСѓ СЃРёР»СѓСЌС‚ РЅРµСЂРѕРІРЅС‹Р№.
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
            // Р§СѓС‚СЊ СЂР°Р·РЅС‹Р№ РѕС‚С‚РµРЅРѕРє Сѓ С‡Р°СЃС‚РµР№ вЂ” РєР°РјРµРЅСЊ РІС‹РіР»СЏРґРёС‚ РЅРµРѕРґРЅРѕСЂРѕРґРЅС‹Рј.
            let v = 0.8 + rnd() * 0.4;
            s.blocks.push(Block::axis(Vec3::new(cx + ox, cy + oy, cz + oz), Vec3::new(hx, hy, hz), base.scale(v)));
        }
    }
    s.camera.origin = Vec3::ZERO;
    s.camera.forward = Vec3::new(0.02, 0.0, 1.0);
    s.camera.fov = 1.25;
    s.star_density = 0.1;
    s
}

// ---------------------------------------------------------------------------
// Корабль из состояния симуляции
// ---------------------------------------------------------------------------

/// Описание корабля, пришедшее из симуляции: где он и как повёрнут.
///
/// Это прямой мост: `EntityState` из протокола → блоки для рендера.
#[derive(Clone, Copy, Debug)]
pub struct ShipPose {
    pub pos: Vec3,
    pub yaw: f64,
    pub pitch: f64,
}

impl ShipPose {
    /// Из состояния сущности (позиция + углы).
    pub fn from_state(e: &ou_protocol::EntityState) -> Self {
        ShipPose { pos: e.pos, yaw: e.yaw as f64, pitch: e.pitch as f64 }
    }
}

/// Локальный «план» корабля: список деталей (центр, половинные размеры, цвет).
///
/// Отдельно от мира, поэтому одна и та же форма используется и для сцен, и для
/// кораблей из симуляции.
fn ship_parts() -> Vec<(Vec3, Vec3, Color)> {
    let hull = Color::rgb(0.74, 0.77, 0.82);
    let hull_dark = Color::rgb(0.42, 0.46, 0.55);
    let accent = Color::rgb(0.16, 0.62, 0.72);
    let glass = Color::rgb(0.12, 0.2, 0.34);
    let engine = Color::rgb(0.35, 0.95, 1.0);

    let mut parts = Vec::with_capacity(40);
    // Корпус вдоль +Z.
    for i in -8..=8i64 {
        let z = 40.0 + i as f64 * 16.0;
        let w = if i.abs() > 6 { 7.0 } else if i.abs() > 3 { 12.0 } else { 15.0 };
        parts.push((
            Vec3::new(0.0, 0.0, z),
            Vec3::new(w, 8.0, 8.5),
            if i % 2 == 0 { hull } else { hull_dark },
        ));
    }
    // Верхний гребень.
    for i in -4..=4i64 {
        parts.push((
            Vec3::new(0.0, 9.0, 55.0 + i as f64 * 16.0),
            Vec3::new(5.0, 2.6, 7.0),
            hull_dark,
        ));
    }
    // Крылья.
    for side in [-1.0, 1.0] {
        for i in 0..4i64 {
            parts.push((
                Vec3::new(side * (22.0 + i as f64 * 16.0), -1.6, 25.0 + i as f64 * 9.0),
                Vec3::new(8.0, 2.4, 26.0),
                if i == 0 { accent } else { hull_dark },
            ));
        }
    }
    // Кабина.
    parts.push((Vec3::new(0.0, 10.5, 82.0), Vec3::new(9.0, 4.8, 18.0), glass));
    // Двигатели.
    for side in [-1.0, 1.0] {
        parts.push((
            Vec3::new(side * 10.0, 0.0, -20.0),
            Vec3::new(6.0, 6.0, 7.5),
            engine,
        ));
    }
    parts
}

/// Построить блоки корабля по его позе в мире.
///
/// Форма корабля задана локально (вдоль +Z), а затем поворачивается и
/// переносится в мировые координаты через [`Mat3`] и `pos`. Именно так это
/// будет работать в UE5: локальная геометрия + трансформ сущности.
pub fn ship_blocks(pose: ShipPose, scale: f64) -> Vec<Block> {
    let rot = Mat3::from_yaw_pitch(pose.yaw, pose.pitch);
    ship_parts()
        .into_iter()
        .map(|(center, half, color)| {
            Block::oriented(pose.pos.add(rot.mul_vec(center.scale(scale))), half.scale(scale), color, rot)
        })
        .collect()
}

/// Планета с рельефом и атмосферой (сокращение для сцен).
pub fn planet(center: Vec3, radius: f64, seed: u64) -> Sphere {
    Sphere {
        center,
        radius,
        color: Color::rgb(0.2, 0.4, 0.6),
        terrain: true,
        emissive: 0.0,
        seed,
        atmosphere: 1.1,
    }
}

/// Звезда: источник света и яркая точка (сокращение для сцен).
pub fn star(center: Vec3, radius: f64) -> Sphere {
    Sphere {
        center,
        radius,
        color: Color::rgb(1.0, 0.93, 0.8),
        terrain: false,
        emissive: 2.6,
        seed: 1,
        atmosphere: 0.0,
    }
}

/// Сцена: мир симуляции. Корабли берутся из состояний, камера — рядом с ними.
///
/// `states` — те же `EntityState`, что сервер шлёт по сети. Это и есть
/// «рендер реального мира»: ничего искусственного, только данные симуляции.
pub fn scene_from_states(states: &[ou_protocol::EntityState], focus_id: u32) -> Scene {
    let mut s = Scene::new();
    // Планета на фоне.
    s.spheres.push(planet(Vec3::new(2200.0, -1600.0, 6000.0), 2100.0, 12345));
    // Звезда-светило.
    let star_pos = Vec3::new(-4800.0, 3000.0, 4200.0);
    s.spheres.push(star(star_pos, 340.0));
    s.light.direction = norm(star_pos);
    s.star_density = 0.09;

    // Корабли: свой — крупнее, чужие — меньше (это визуальное различие).
    for st in states {
        let scale = if st.id == focus_id { 0.95 } else { 0.75 };
        s.blocks.extend(ship_blocks(ShipPose::from_state(st), scale));
    }

    // Камера смотрит на корабль игрока сзади-сверху.
    if let Some(me) = states.iter().find(|e| e.id == focus_id) {
        let rot = Mat3::from_yaw_pitch(me.yaw as f64, me.pitch as f64);
        let back = Vec3::new(0.0, 70.0, -330.0);
        s.camera.origin = me.pos.add(rot.mul_vec(back));
        s.camera.forward = rot.mul_vec(Vec3::new(0.0, -0.08, 1.0));
    } else if let Some(first) = states.first() {
        s.camera.origin = first.pos.add(Vec3::new(0.0, 60.0, -320.0));
        s.camera.forward = Vec3::new(0.0, -0.08, 1.0);
    }
    s.camera.fov = 1.0;
    s
}

// ---------------------------------------------------------------------------
// Анимация
// ---------------------------------------------------------------------------

/// Сцена «воксельная планета»: поверхность из вокселей, видно искривление
/// кубической сетки в сферу и уровни рельефа (океан, суша, горы, снег).
pub fn scene_voxel_planet() -> Scene {
    use crate::voxel::{Lod, VoxelPlanet};
    let mut s = Scene::new();
    let radius = 1600.0;
    // Генерируем планету среднего LOD (видно отдельные воксели).
    let vp = VoxelPlanet::generate(Vec3::ZERO, radius, 0xC0FFEE, Lod::Near);

    // Твёрдое ядро планеты: плотная сфера чуть ниже воксельного слоя.
    // Физически это «тело планеты» под корой, а технически — гарантия, что
    // между вокселями соседних граней куба не будет щелей в космос.
    s.spheres.push(Sphere {
        center: vp.center,
        radius: radius - radius * 0.025,
        color: Color::rgb(0.06, 0.14, 0.30),
        terrain: false,
        emissive: 0.0,
        seed: 0,
        atmosphere: 1.3,
    });

    // Воксельные блоки поверхности (и суша, и вода).
    s.blocks.extend(vp.to_blocks());

    // Свет — звезда сверху-слева.
    let star_pos = Vec3::new(-9000.0, 7000.0, 6000.0);
    s.spheres.push(star(star_pos, 420.0));
    s.light.direction = norm(star_pos);
    s.light.intensity = 1.25;
    s.star_density = 0.08;

    // Камера: видно шарообразный горизонт и искривление сетки.
    let distance = radius * 2.4;
    let cam_dir = norm(Vec3::new(-0.35, 0.30, 1.0));
    s.camera.origin = cam_dir.scale(distance);
    s.camera.forward = norm(Vec3::ZERO.sub(s.camera.origin));
    s.camera.fov = 1.0;
    s
}

/// Сцена сравнения LOD: слева грубая планета, справа детальная.
pub fn scene_voxel_lod_compare() -> Scene {
    use crate::voxel::{Lod, VoxelPlanet};
    let mut s = Scene::new();
    let radius = 900.0;

    // Грубая (далеко) — слева.
    let far = VoxelPlanet::generate(Vec3::new(-1500.0, 0.0, 3500.0), radius, 0xC0FFEE, Lod::Far);
    s.blocks.extend(far.to_blocks());
    // Детальная (близко) — справа.
    let near = VoxelPlanet::generate(Vec3::new(1500.0, 0.0, 3500.0), radius, 0xC0FFEE, Lod::Close);
    s.blocks.extend(near.to_blocks());

    let star_pos = Vec3::new(-6000.0, 5000.0, 4000.0);
    s.spheres.push(star(star_pos, 300.0));
    s.light.direction = norm(star_pos);
    s.light.intensity = 1.2;
    s.star_density = 0.08;

    s.camera.origin = Vec3::new(0.0, 200.0, -1800.0);
    s.camera.forward = Vec3::new(0.0, 0.05, 1.0);
    s.camera.fov = 1.2;
    s
}
/// Собрать APNG-подобную последовательность кадров в одно изображение-лист
/// (контактный лист), чтобы показать движение одним файлом.
pub fn contact_sheet(frames: &[(u32, u32, Vec<u8>)], cols: usize) -> Option<(u32, u32, Vec<u8>)> {
    if frames.is_empty() {
        return None;
    }
    let (fw, fh, _) = (frames[0].0, frames[0].1, frames[0].2.len());
    let cols = cols.max(1);
    let rows = (frames.len() + cols - 1) / cols;
    let w = fw * cols as u32;
    let h = fh * rows as u32;
    let mut out = vec![0u8; w as usize * h as usize * 3];
    for (i, (fw_i, fh_i, data)) in frames.iter().enumerate() {
        let cx = (i % cols) as u32;
        let cy = (i / cols) as u32;
        for y in 0..*fh_i {
            let dst_y = cy * fh + y;
            for x in 0..*fw_i {
                let src = ((y * *fw_i + x) * 3) as usize;
                let dst = ((dst_y * w + (cx * fw + x)) * 3) as usize;
                if dst + 3 <= out.len() && src + 3 <= data.len() {
                    out[dst] = data[src];
                    out[dst + 1] = data[src + 1];
                    out[dst + 2] = data[src + 2];
                }
            }
        }
    }
    Some((w, h, out))
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
        let b = Block::axis(Vec3::new(0.0, 0.0, 50.0), Vec3::new(5.0, 5.0, 5.0), Color::rgb(1.0, 1.0, 1.0));
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
        // Р‘РµСЂС‘Рј РєРѕРѕСЂРґРёРЅР°С‚С‹ Р·РІРµР·РґС‹ РїСЂСЏРјРѕ РёР· СЃС†РµРЅС‹ (РѕРЅРё РјРµРЅСЏСЋС‚СЃСЏ РІРјРµСЃС‚Рµ СЃ РЅРµР№).
        let star = s
            .spheres
            .iter()
            .find(|sp| sp.emissive > 0.0)
            .expect("РІ СЃС†РµРЅРµ РґРѕР»Р¶РЅР° Р±С‹С‚СЊ Р·РІРµР·РґР°");
        let dir = norm(star.center);
        let c = trace(&s, Vec3::ZERO, dir, 1.0);
        let lum = c.r + c.g + c.b;
        assert!(lum > 0.5, "Р·РІРµР·РґР° РґРѕР»Р¶РЅР° Р±С‹С‚СЊ СЏСЂРєРѕР№, РїРѕР»СѓС‡РµРЅРѕ {lum}");
    }

    #[test]
    fn background_has_stars_or_empty_space() {
        let s = Scene::new();
        // РЎРјРѕС‚СЂРёРј РІ В«РїСѓСЃС‚РѕРµВ» РЅР°РїСЂР°РІР»РµРЅРёРµ.
        let c = trace(&s, Vec3::ZERO, Vec3::new(0.1, 0.2, 0.97), 1.0);
        // РљРѕСЃРјРѕСЃ С‚С‘РјРЅС‹Р№, РЅРѕ РЅРµ РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ Р°Р±СЃРѕР»СЋС‚РЅРѕ РЅСѓР»РµРІС‹Рј.
        assert!(c.r < 1.0 && c.g < 1.0 && c.b < 1.0);
    }

    #[test]
    fn noise_is_in_unit_range() {
        for i in 0..200 {
            let p = Vec3::new(i as f64 * 3.1, i as f64 * 1.7, i as f64 * 0.9);
            let n = fbm(p, 42);
            assert!((0.0..=1.0).contains(&n), "С€СѓРј РІРЅРµ РґРёР°РїР°Р·РѕРЅР°: {n}");
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

    // --- ориентация и корабль из симуляции ---

    #[test]
    fn rotation_changes_obb_hit() {
        // Блок, повёрнутый на 90° вокруг Y, должен по-разному пересекаться
        // лучом, идущим по X, по сравнению с невыровненным блоком.
        let rot = Mat3::from_yaw_pitch(std::f64::consts::FRAC_PI_2, 0.0);
        let b = Block::oriented(Vec3::new(0.0, 0.0, 50.0), Vec3::new(20.0, 2.0, 2.0), Color::rgb(1.0, 1.0, 1.0), rot);
        // До поворота длинная ось была вдоль X; после поворота — вдоль Z.
        // Луч по Z теперь должен попадать.
        assert!(b.hit_ex(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0)).is_some());
    }

    #[test]
    fn identity_rotation_matches_axis_block() {
        let a = Block::axis(Vec3::new(0.0, 0.0, 50.0), Vec3::new(5.0, 5.0, 5.0), Color::rgb(1.0, 1.0, 1.0));
        let b = Block::oriented(Vec3::new(0.0, 0.0, 50.0), Vec3::new(5.0, 5.0, 5.0), Color::rgb(1.0, 1.0, 1.0), Mat3::IDENTITY);
        let ra = a.hit_ex(Vec3::ZERO, Vec3::new(0.1, 0.0, 1.0));
        let rb = b.hit_ex(Vec3::ZERO, Vec3::new(0.1, 0.0, 1.0));
        assert_eq!(ra.is_some(), rb.is_some());
        if let (Some((ta, _)), Some((tb, _))) = (ra, rb) {
            assert!((ta - tb).abs() < 1e-9);
        }
    }

    #[test]
    fn ship_blocks_follow_position_and_rotation() {
        let pose = ShipPose { pos: Vec3::new(1000.0, 50.0, -300.0), yaw: 0.7, pitch: 0.1 };
        let blocks = ship_blocks(pose, 1.0);
        assert!(blocks.len() > 20, "корабль должен состоять из многих блоков");
        // Все блоки рядом с позицией корабля (не разлетелись).
        for b in &blocks {
            let d = b.center.distance(pose.pos);
            assert!(d < 200.0, "блок улетел от корабля: {d}");
        }
        // Поворот меняет положение: без поворота блоки стояли бы по-другому.
        let straight = ship_blocks(ShipPose { yaw: 0.0, ..pose }, 1.0);
        assert_ne!(blocks[0].center.to_wire(), straight[0].center.to_wire());
    }

    #[test]
    fn scene_from_states_places_camera_at_focus() {
        let states = vec![
            ou_protocol::EntityState { id: 1, pos: Vec3::new(0.0, 0.0, 0.0), vel: Vec3::ZERO, yaw: 0.0, pitch: 0.0 },
            ou_protocol::EntityState { id: 2, pos: Vec3::new(0.0, 0.0, -500.0), vel: Vec3::ZERO, yaw: 0.0, pitch: 0.0 },
        ];
        let s = scene_from_states(&states, 1);
        // Камера — позади корабля 1 (по −Z), значит её z меньше нуля.
        assert!(s.camera.origin.z < 0.0, "камера должна быть позади: {:?}", s.camera.origin);
        // Оба корабля дали блоки.
        assert!(s.blocks.len() > 40);
    }

    #[test]
    fn render_from_states_is_deterministic() {
        let states = vec![ou_protocol::EntityState {
            id: 1,
            pos: Vec3::new(0.0, 0.0, 0.0),
            vel: Vec3::ZERO,
            yaw: 0.3,
            pitch: 0.05,
        }];
        let a = render(&scene_from_states(&states, 1), 48, 32);
        let b = render(&scene_from_states(&states, 1), 48, 32);
        assert_eq!(a, b);
    }

    #[test]
    fn contact_sheet_tiles_frames() {
        // Четыре кадра 2×2 → лист 4×4 при 2 колонках.
        let frame = |v: u8| (2u32, 2u32, vec![v; 2 * 2 * 3]);
        let frames = vec![frame(10), frame(20), frame(30), frame(40)];
        let (w, h, data) = contact_sheet(&frames, 2).unwrap();
        assert_eq!((w, h), (4, 4));
        assert_eq!(data.len(), 4 * 4 * 3);
        // Верхний левый пиксель — из первого кадра.
        assert_eq!(data[0], 10);
        // Пиксель во второй колонке верхней строки — из второго кадра.
        let idx = (0 * 4 + 2) * 3;
        assert_eq!(data[idx], 20);
    }

    #[test]
    fn contact_sheet_handles_empty() {
        assert!(contact_sheet(&[], 2).is_none());
    }
}
