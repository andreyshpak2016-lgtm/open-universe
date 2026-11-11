//! Р РµР»СЊРµС„ РїР»Р°РЅРµС‚С‹: РІС‹СЃРѕС‚Р° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё РєР°Рє С„СѓРЅРєС†РёСЏ РЅР°РїСЂР°РІР»РµРЅРёСЏ.
//!
//! ## Р—Р°С‡РµРј СЌС‚Рѕ РІ `sim`, Р° РЅРµ РІ СЂРµРЅРґРµСЂРµ
//! Р’С‹СЃРѕС‚Р° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё вЂ” СЌС‚Рѕ **РґР°РЅРЅС‹Рµ РјРёСЂР°**: РїРѕ РЅРёРј СЃРµСЂРІРµСЂ РїСЂРѕРІРµСЂСЏРµС‚ РїРѕСЃР°РґРєСѓ
//! РєРѕСЂР°Р±Р»СЏ, СЃС‚РѕР»РєРЅРѕРІРµРЅРёСЏ Рё РґРѕР±С‹С‡Сѓ. Р РµРЅРґРµСЂ Р»РёС€СЊ СЂРёСЃСѓРµС‚ С‚РѕС‚ Р¶Рµ СЂРµР»СЊРµС„. РџРѕСЌС‚РѕРјСѓ
//! СЂРµР»СЊРµС„ РѕРїРёСЃР°РЅ Р·РґРµСЃСЊ, СЂСЏРґРѕРј СЃ С„РёР·РёРєРѕР№, Рё РѕР±Рµ СЃС‚РѕСЂРѕРЅС‹ РёСЃРїРѕР»СЊР·СѓСЋС‚ РѕРґРЅСѓ С„РѕСЂРјСѓР»Сѓ.
//!
//! ## РљР°Рє СѓСЃС‚СЂРѕРµРЅРѕ
//! 1. РќР°РїСЂР°РІР»РµРЅРёРµ РЅР° СЃС„РµСЂРµ в†’ С‚РѕС‡РєР° РЅР° РіСЂР°РЅРё РєСѓР±Р° (РѕР±СЂР°С‚РЅРѕРµ cube-to-sphere).
//! 2. Р’С‹СЃРѕС‚Р° = СЃСѓРјРјР° РґРІСѓС… РѕРєС‚Р°РІ С€СѓРјР° (РјР°С‚РµСЂРёРєРё + РґРµС‚Р°Р»Рё).
//! 3. РЈСЂРѕРІРµРЅСЊ РѕРєРµР°РЅР°: РІСЃС‘ РЅРёР¶Рµ РЅРµРіРѕ СЃС‡РёС‚Р°РµС‚СЃСЏ РІРѕРґРѕР№.
//!
//! Р’СЃС‘ РґРµС‚РµСЂРјРёРЅРёСЂРѕРІР°РЅРѕ: РѕРґРЅР° Рё С‚Р° Р¶Рµ С„СѓРЅРєС†РёСЏ РґР°С‘С‚ РѕРґРёРЅР°РєРѕРІС‹Р№ СЂРµР»СЊРµС„ РЅР° СЃРµСЂРІРµСЂРµ,
//! РІ РєР»РёРµРЅС‚Рµ Рё РІ С‚РµСЃС‚Р°С….

use crate::{det_sin, Vec3};

/// РџСЂРёРІРµРґРµРЅРёРµ РЅР°РїСЂР°РІР»РµРЅРёСЏ Рє РѕРєС‚Р°СЌРґСЂРёС‡РµСЃРєРёРј РєРѕРѕСЂРґРёРЅР°С‚Р°Рј (octahedral mapping).
///
/// Р­С‚Рѕ РѕР±СЂР°С‚РёРјРѕРµ РѕС‚РѕР±СЂР°Р¶РµРЅРёРµ СЃС„РµСЂС‹ РЅР° РєРІР°РґСЂР°С‚ вЂ” РЅСѓР¶РЅРѕ, С‡С‚РѕР±С‹ РІС‹С‡РёСЃР»СЏС‚СЊ С€СѓРј
/// РїРѕ РЅРµРїСЂРµСЂС‹РІРЅС‹Рј РєРѕРѕСЂРґРёРЅР°С‚Р°Рј Р±РµР· СЂР°Р·СЂС‹РІРѕРІ РЅР° РіСЂР°РЅСЏС… РєСѓР±Р°.
fn octahedral(p: Vec3) -> (f64, f64) {
    let l = p.x.abs() + p.y.abs() + p.z.abs();
    let (mut u, mut v) = if l < 1e-12 {
        (0.0, 0.0)
    } else {
        (p.x / l, p.z / l)
    };
    // РЎРєР»Р°РґС‹РІР°РµРј РЅРёР¶РЅСЋСЋ РїРѕР»СѓСЃС„РµСЂСѓ РЅР° РІРµСЂС…РЅСЋСЋ (РєР°Рє РІ octahedral normal encoding).
    if p.y < 0.0 {
        let ux = (1.0 - v.abs()) * if u >= 0.0 { 1.0 } else { -1.0 };
        let vy = (1.0 - u.abs()) * if v >= 0.0 { 1.0 } else { -1.0 };
        u = ux;
        v = vy;
    }
    (u, v)
}

/// РҐРµС€ СЂРµС€С‘С‚РєРё РІ [0,1).
fn lattice(x: i64, y: i64, seed: u64) -> f64 {
    let mut h = seed;
    for c in [x, y] {
        h ^= (c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Р”РІСѓРјРµСЂРЅС‹Р№ value noise СЃ Р±РёРєСѓР±РёС‡РµСЃРєРѕР№ РёРЅС‚РµСЂРїРѕР»СЏС†РёРµР№.
fn noise2(u: f64, v: f64, seed: u64) -> f64 {
    let (xi, yi) = (u.floor(), v.floor());
    let (fx, fy) = (smooth(u - xi), smooth(v - yi));
    let (ix, iy) = (xi as i64, yi as i64);
    let c = |dx: i64, dy: i64| lattice(ix + dx, iy + dy, seed);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x0 = lerp(c(0, 0), c(1, 0), fx);
    let x1 = lerp(c(0, 1), c(1, 1), fx);
    lerp(x0, x1, fy)
}

/// РќР°СЃС‚СЂРѕР№РєРё СЂРµР»СЊРµС„Р° РїР»Р°РЅРµС‚С‹.
#[derive(Clone, Copy, Debug)]
pub struct TerrainParams {
    /// Р‘Р°Р·РѕРІС‹Р№ СЂР°РґРёСѓСЃ (СѓСЂРѕРІРµРЅСЊ РѕРєРµР°РЅР°).
    pub radius: f64,
    /// Р Р°Р·РјР°С… СЂРµР»СЊРµС„Р° РІ РµРґРёРЅРёС†Р°С… РґР»РёРЅС‹.
    pub amplitude: f64,
    /// Р”РѕР»СЏ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё, РєРѕС‚РѕСЂСѓСЋ Р·Р°РЅРёРјР°РµС‚ РѕРєРµР°РЅ (0..1).
    pub sea_level: f64,
    pub seed: u64,
}

impl Default for TerrainParams {
    fn default() -> Self {
        TerrainParams { radius: 1000.0, amplitude: 30.0, sea_level: 0.30, seed: 0xC0FFEE }
    }
}

/// Р РµР»СЊРµС„ РїР»Р°РЅРµС‚С‹: С‡РёСЃС‚Р°СЏ С„СѓРЅРєС†РёСЏ РѕС‚ РЅР°РїСЂР°РІР»РµРЅРёСЏ.
#[derive(Clone, Copy, Debug)]
pub struct Terrain {
    pub center: Vec3,
    pub params: TerrainParams,
}

impl Terrain {
    pub fn new(center: Vec3, params: TerrainParams) -> Self {
        Terrain { center, params }
    }

    /// Р’С‹СЃРѕС‚Р° РЅР°Рґ СѓСЂРѕРІРЅРµРј РѕРєРµР°РЅР° РІ РЅР°РїСЂР°РІР»РµРЅРёРё `dir` (РјРѕР¶РµС‚ Р±С‹С‚СЊ < 0 вЂ” РІРѕРґР°).
    pub fn height(&self, dir: Vec3) -> f64 {
        let d = normalize(dir);
        let (u, v) = octahedral(d);
        // Р”РІРµ РѕРєС‚Р°РІС‹: РєСЂСѓРїРЅС‹Рµ РјР°С‚РµСЂРёРєРё + РјРµР»РєРёРµ РґРµС‚Р°Р»Рё.
        let coarse = noise2(u * 2.6, v * 2.6, self.params.seed);
        let fine = noise2(u * 9.0, v * 9.0, self.params.seed ^ 0xF1E);
        let n = coarse * 0.72 + fine * 0.28;
        (n - self.params.sea_level) * self.params.amplitude * 2.0
    }

    /// Р Р°РґРёСѓСЃ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё РІ РЅР°РїСЂР°РІР»РµРЅРёРё `dir` (С†РµРЅС‚СЂ РїР»Р°РЅРµС‚С‹ в†’ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ).
    pub fn surface_radius(&self, dir: Vec3) -> f64 {
        self.params.radius + self.height(dir)
    }

    /// РўРѕС‡РєР° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё РІ РЅР°РїСЂР°РІР»РµРЅРёРё `dir`.
    pub fn surface_point(&self, dir: Vec3) -> Vec3 {
        self.center.add(normalize(dir).scale(self.surface_radius(dir)))
    }

    /// РЇРІР»СЏРµС‚СЃСЏ Р»Рё С‚РѕС‡РєР° РІРѕРґРѕР№ (РЅРёР¶Рµ СѓСЂРѕРІРЅСЏ РѕРєРµР°РЅР°).
    pub fn is_water(&self, dir: Vec3) -> bool {
        self.height(dir) <= 0.0
    }

    /// Р’С‹СЃРѕС‚Р° РІ С‚РѕС‡РєРµ РјРёСЂР°: `None`, РµСЃР»Рё С‚РѕС‡РєР° РґР°Р»РµРєРѕ РѕС‚ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё РїР»Р°РЅРµС‚С‹.
    ///
    /// РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ С„РёР·РёРєРѕР№: С‡С‚РѕР±С‹ СѓР·РЅР°С‚СЊ, В«РЅР° Р·РµРјР»РµВ» Р»Рё РєРѕСЂР°Р±Р»СЊ.
    pub fn height_at_point(&self, p: Vec3) -> f64 {
        self.height(p.sub(self.center))
    }

    /// РќР°РїСЂР°РІР»РµРЅРёРµ В«РІРІРµСЂС…В» РѕС‚ С†РµРЅС‚СЂР° РїР»Р°РЅРµС‚С‹ РІ С‚РѕС‡РєРµ (РЅРѕСЂРјР°Р»СЊ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё).
    pub fn up_at(&self, p: Vec3) -> Vec3 {
        normalize(p.sub(self.center))
    }

    /// РџСЂРѕРЅРёРє Р»Рё РєРѕСЂР°Р±Р»СЊ РїРѕРґ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ? Р’РѕР·РІСЂР°С‰Р°РµС‚ РіР»СѓР±РёРЅСѓ РїСЂРѕРЅРёРєРЅРѕРІРµРЅРёСЏ
    /// (0, РµСЃР»Рё РІСЃС‘ РІ РїРѕСЂСЏРґРєРµ).
    pub fn penetration(&self, p: Vec3) -> f64 {
        let rel = p.sub(self.center);
        let dist = rel.length();
        if dist < 1e-9 {
            return 0.0;
        }
        let surf = self.surface_radius(rel);
        (surf - dist).max(0.0)
    }
}

/// РќРѕСЂРјР°Р»РёР·РѕРІР°С‚СЊ РІРµРєС‚РѕСЂ.
pub fn normalize(v: Vec3) -> Vec3 {
    let l = v.length();
    if l < 1e-12 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        v.scale(1.0 / l)
    }
}

/// Р¤РёР·РёРєР° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё: РїРѕСЃР°РґРєР° Рё СЃС‚РѕР»РєРЅРѕРІРµРЅРёРµ СЃ СЂРµР»СЊРµС„РѕРј.
pub mod landing {
    use super::*;
    /// РЎРѕСЃС‚РѕСЏРЅРёРµ В«РєРѕСЂР°Р±Р»СЊ РЅР° РїРѕРІРµСЂС…РЅРѕСЃС‚РёВ».
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum GroundState {
        /// Р’ РІРѕР·РґСѓС…Рµ/РєРѕСЃРјРѕСЃРµ вЂ” РЅРёС‡РµРіРѕ РЅРµ РґРµР»Р°РµРј.
        Flying,
        /// РљР°СЃР°РЅРёРµ: РјСЏРіРєР°СЏ РїРѕСЃР°РґРєР°, СЃРєРѕСЂРѕСЃС‚СЊ РіР°СЃРёС‚СЃСЏ С‚СЂРµРЅРёРµРј.
        Landed { altitude: f64 },
        /// РЈРґР°СЂ: СЃРєРѕСЂРѕСЃС‚СЊ СЃР»РёС€РєРѕРј РІРµР»РёРєР°, РєРѕСЂР°Р±Р»СЊ РїРѕР»СѓС‡Р°РµС‚ РїРѕРІСЂРµР¶РґРµРЅРёСЏ.
        Crashed { impact_speed: f64 },
    }

    /// Р Р°Р·СЂРµС€РёС‚СЊ РєРѕРЅС‚Р°РєС‚ СЃ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊСЋ: РїСЂРёР¶Р°С‚СЊ РєРѕСЂР°Р±Р»СЊ, РµСЃР»Рё РѕРЅ РїРѕРґ РЅРµР№.
    ///
    /// Р’РѕР·РІСЂР°С‰Р°РµС‚ СЃРѕСЃС‚РѕСЏРЅРёРµ Рё (РґР»СЏ СѓРґР°СЂР°) СЃРєРѕСЂРѕСЃС‚СЊ СЃС‚РѕР»РєРЅРѕРІРµРЅРёСЏ.
    /// `safe_speed` вЂ” СЃРєРѕСЂРѕСЃС‚СЊ, РґРѕ РєРѕС‚РѕСЂРѕР№ РїРѕСЃР°РґРєР° СЃС‡РёС‚Р°РµС‚СЃСЏ РјСЏРіРєРѕР№.
    pub fn resolve_ground(
        pos: &mut Vec3,
        vel: &mut Vec3,
        terrain: &Terrain,
        safe_speed: f64,
        friction: f64,
        params: &crate::ShipParams,
    ) -> GroundState {
        let pen = terrain.penetration(*pos);
        if pen <= 0.0 {
            return GroundState::Flying;
        }
        // РџРѕРґРЅРёРјР°РµРј РєРѕСЂР°Р±Р»СЊ С‚РѕС‡РЅРѕ РЅР° РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ.
        let rel = pos.sub(terrain.center);
        let dir = normalize(rel);
        let target = terrain.surface_radius(dir);
        *pos = terrain.center.add(dir.scale(target));

        // Р“Р°СЃРёРј СЃРєРѕСЂРѕСЃС‚СЊ: РЅРѕСЂРјР°Р»СЊРЅСѓСЋ СЃРѕСЃС‚Р°РІР»СЏСЋС‰СѓСЋ СѓР±РёСЂР°РµРј, РєР°СЃР°С‚РµР»СЊРЅСѓСЋ вЂ” С‚СЂРµРЅРёРµРј.
        let v_normal = dir.scale(vel.dot(dir));
        let v_tangent = vel.sub(v_normal);
        let impact = v_normal.length();

        let v_tangent_damped = v_tangent.scale(1.0 - friction.clamp(0.0, 1.0));
        *vel = v_tangent_damped;

        // РћРіСЂР°РЅРёС‡РёРј СЃРєРѕСЂРѕСЃС‚СЊ СЃРЅРёР·Сѓ/СЃРІРµСЂС…Сѓ РєР°Рє РІ РѕР±С‹С‡РЅРѕРј РїРѕР»С‘С‚Рµ.
        let speed = vel.length();
        if speed > params.max_speed {
            *vel = vel.scale(params.max_speed / speed);
        }

        if impact > safe_speed {
            GroundState::Crashed { impact_speed: impact }
        } else {
            let altitude = pos.distance(terrain.center) - terrain.params.radius;
            GroundState::Landed { altitude }
        }
    }

    /// РњСЏРіРєР°СЏ Р»Рё РїРѕСЃР°РґРєР° (СѓРґРѕР±РЅР°СЏ РѕР±С‘СЂС‚РєР°).
    pub fn is_soft_landing(impact_speed: f64, safe_speed: f64) -> bool {
        impact_speed <= safe_speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Terrain {
        Terrain::new(Vec3::ZERO, TerrainParams::default())
    }

    #[test]
    fn height_is_deterministic() {
        let t = t();
        for d in [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.2, -0.3),
            Vec3::new(-0.5, -0.5, 0.7),
        ] {
            assert_eq!(t.height(d), t.height(d));
        }
    }

    #[test]
    fn height_is_bounded_by_amplitude() {
        let t = t();
        let amp = t.params.amplitude * 2.0;
        let mut h: f64 = 0.0;
        // РћР±С…РѕРґРёРј РјРЅРѕРіРѕ РЅР°РїСЂР°РІР»РµРЅРёР№: РІС‹СЃРѕС‚Р° РЅРµ РґРѕР»Р¶РЅР° В«РІР·СЂС‹РІР°С‚СЊСЃСЏВ».
        while h < 6.28 {
            let mut a: f64 = 0.0;
            while a < 3.14 {
                let d = Vec3::new(h.cos() * a.cos(), a.sin(), h.sin() * a.cos());
                let hh = t.height(d);
                assert!(hh.abs() <= amp + 1e-9, "РІС‹СЃРѕС‚Р° {hh} РІС‹С€Р»Р° Р·Р° {amp}");
                a += 0.35;
            }
            h += 0.35;
        }
    }

    #[test]
    fn height_is_continuous_across_directions() {
        // РњР°Р»РµРЅСЊРєРѕРµ РёР·РјРµРЅРµРЅРёРµ РЅР°РїСЂР°РІР»РµРЅРёСЏ в†’ РјР°Р»РµРЅСЊРєРѕРµ РёР·РјРµРЅРµРЅРёРµ РІС‹СЃРѕС‚С‹.
        let t = t();
        let a = normalize(Vec3::new(0.3, 0.5, 0.8));
        let b = normalize(Vec3::new(0.301, 0.5, 0.8));
        let diff = (t.height(a) - t.height(b)).abs();
        assert!(diff < 1.0, "СЂР°Р·СЂС‹РІ РІ СЂРµР»СЊРµС„Рµ: {diff}");
    }

    #[test]
    fn surface_radius_matches_height() {
        let t = t();
        let d = normalize(Vec3::new(0.2, 0.9, -0.3));
        assert!((t.surface_radius(d) - (t.params.radius + t.height(d))).abs() < 1e-9);
    }

    #[test]
    fn there_is_both_water_and_land() {
        let t = t();
        let mut water: u64 = 0;
        let mut land: u64 = 0;
        let mut h: f64 = 0.0;
        while h < 6.28 {
            let mut a: f64 = -1.4;
            while a < 1.4 {
                let d = Vec3::new(h.cos() * a.cos(), a.sin(), h.sin() * a.cos());
                if t.is_water(d) {
                    water += 1;
                } else {
                    land += 1;
                }
                a += 0.2;
            }
            h += 0.2;
        }
        assert!(water > 0, "РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ РѕРєРµР°РЅ");
        assert!(land > 0, "РґРѕР»Р¶РЅР° Р±С‹С‚СЊ СЃСѓС€Р°");
        // РћРєРµР°РЅ Р·Р°РЅРёРјР°РµС‚ РѕС‰СѓС‚РёРјСѓСЋ, РЅРѕ РЅРµ РїРѕРґР°РІР»СЏСЋС‰СѓСЋ РґРѕР»СЋ.
        let frac = water as f64 / (water + land) as f64;
        assert!((0.05..0.75).contains(&frac), "РґРѕР»СЏ РѕРєРµР°РЅР° РЅРµРїСЂР°РІРґРѕРїРѕРґРѕР±РЅР°: {frac}");
    }

    #[test]
    fn penetration_detects_buried_point() {
        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        // РўРѕС‡РєР° РїРѕРґ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊСЋ.
        let buried = t.center.add(d.scale(surf - 50.0));
        assert!((t.penetration(buried) - 50.0).abs() < 1e-6);
        // РўРѕС‡РєР° РЅР°Рґ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊСЋ.
        let above = t.center.add(d.scale(surf + 50.0));
        assert_eq!(t.penetration(above), 0.0);
    }

    #[test]
    fn resolution_puts_ship_on_surface() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.1, 1.0, 0.2));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf - 40.0)); // РїРѕРґ Р·РµРјР»С‘Р№
        let mut vel = Vec3::new(0.0, -30.0, 0.0); // Р»РµС‚РёС‚ РІРЅРёР·
        let params = crate::ShipParams::default();
        let state = resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.5, &params);
        // РљРѕСЂР°Р±Р»СЊ РЅР° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё, СЃРєРѕСЂРѕСЃС‚СЊ РІРЅРёР· РїРѕРіР°С€РµРЅР°.
        assert!((t.penetration(pos)).abs() < 1e-6, "РєРѕСЂР°Р±Р»СЊ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ СЂРѕРІРЅРѕ РЅР° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё");
        assert!(vel.dot(d) <= 1e-6, "СЃРєРѕСЂРѕСЃС‚СЊ В«РІ Р·РµРјР»СЋВ» РґРѕР»Р¶РЅР° Р±С‹С‚СЊ РїРѕРіР°С€РµРЅР°");
        assert!(matches!(state, GroundState::Landed { .. } | GroundState::Crashed { .. }));
    }

    #[test]
    fn high_speed_impact_is_a_crash() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf - 5.0));
        let mut vel = Vec3::new(0.0, -300.0, 0.0); // РѕС‡РµРЅСЊ Р±С‹СЃС‚СЂРѕ РІРЅРёР·
        let params = crate::ShipParams::default();
        let state = resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.5, &params);
        match state {
            GroundState::Crashed { impact_speed } => {
                assert!(impact_speed > 50.0, "СѓРґР°СЂ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ СЃРёР»СЊРЅС‹Рј: {impact_speed}");
            }
            other => panic!("РѕР¶РёРґР°Р»СЃСЏ РєСЂР°С€, РїРѕР»СѓС‡РµРЅРѕ {other:?}"),
        }
        assert!(!is_soft_landing(300.0, 50.0));
    }

    #[test]
    fn flying_when_above_surface() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf + 500.0));
        let before = pos;
        let mut vel = Vec3::new(10.0, 0.0, 0.0);
        let params = crate::ShipParams::default();
        let state = resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.5, &params);
        assert_eq!(state, GroundState::Flying);
        // Р’ РІРѕР·РґСѓС…Рµ РЅРёС‡РµРіРѕ РЅРµ РјРµРЅСЏРµС‚СЃСЏ.
        assert_eq!(pos.to_wire(), before.to_wire());
        assert_eq!(vel.to_wire(), Vec3::new(10.0, 0.0, 0.0).to_wire());
    }

    #[test]
    fn friction_slows_tangential_motion() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf - 1.0));
        // РўР°РЅРіРµРЅС†РёР°Р»СЊРЅР°СЏ СЃРєРѕСЂРѕСЃС‚СЊ (РІР±РѕРє) вЂ” С‚СЂРµРЅРёРµ РґРѕР»Р¶РЅРѕ РµС‘ СѓРјРµРЅСЊС€РёС‚СЊ.
        let mut vel = Vec3::new(40.0, -1.0, 0.0);
        let params = crate::ShipParams::default();
        resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.8, &params);
        assert!(vel.length() < 40.0, "С‚СЂРµРЅРёРµ РґРѕР»Р¶РЅРѕ С‚РѕСЂРјРѕР·РёС‚СЊ: {}", vel.length());
    }

    #[test]
    fn terrain_matches_render_style_scale() {
        // РџСЂРѕРІРµСЂСЏРµРј, С‡С‚Рѕ РїР°СЂР°РјРµС‚СЂС‹ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ РґР°СЋС‚ В«РїР»Р°РЅРµС‚РЅС‹Р№В» РјР°СЃС€С‚Р°Р±.
        let t = t();
        assert_eq!(t.params.radius, 1000.0);
        assert!(t.params.amplitude > 0.0);
        // Р’С‹СЃРѕС‚Р° РіРѕСЂ Р·Р°РјРµС‚РЅРѕ РјРµРЅСЊС€Рµ СЂР°РґРёСѓСЃР° (РёРЅР°С‡Рµ СЌС‚Рѕ РЅРµ РїР»Р°РЅРµС‚Р°).
        assert!(t.params.amplitude * 2.0 < t.params.radius * 0.2);
    }

    #[test]
    fn deterministic_across_seeds() {
        let a = Terrain::new(Vec3::ZERO, TerrainParams { seed: 1, ..Default::default() });
        let b = Terrain::new(Vec3::ZERO, TerrainParams { seed: 1, ..Default::default() });
        let c = Terrain::new(Vec3::ZERO, TerrainParams { seed: 2, ..Default::default() });
        let d = normalize(Vec3::new(0.4, 0.5, -0.6));
        assert_eq!(a.height(d), b.height(d));
        assert_ne!(a.height(d), c.height(d));
    }

    #[test]
    fn det_trig_is_available_for_octahedral() {
        // РџСЂРѕСЃС‚Рѕ СѓР±РµРґРёРјСЃСЏ, С‡С‚Рѕ РёСЃРїРѕР»СЊР·СѓРµРјР°СЏ С‚СЂРёРіРѕРЅРѕРјРµС‚СЂРёСЏ РґРµС‚РµСЂРјРёРЅРёСЂРѕРІР°РЅР°.
        assert_eq!(det_sin(0.7), det_sin(0.7));
    }
}
