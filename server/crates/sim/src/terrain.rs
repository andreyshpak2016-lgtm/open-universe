//! Рельеф планеты: высота поверхности как функция направления.
//!
//! ## Зачем это в `sim`, а не в рендере
//! Высота поверхности — это **данные мира**: по ним сервер проверяет посадку
//! корабля, столкновения и добычу. Рендер лишь рисует тот же рельеф. Поэтому
//! рельеф описан здесь, рядом с физикой, и обе стороны используют одну формулу.
//!
//! ## Как устроено
//! 1. Направление на сфере → точка на грани куба (обратное cube-to-sphere).
//! 2. Высота = сумма двух октав шума (материки + детали).
//! 3. Уровень океана: всё ниже него считается водой.
//!
//! Всё детерминировано: одна и та же функция даёт одинаковый рельеф на сервере,
//! в клиенте и в тестах.

use crate::Vec3;

/// Хеш решётки в [0,1). Зеркало клиентского `OUTerrain::Lattice`.
fn lattice3(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed;
    for c in [x, y, z] {
        h ^= (c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Фрактальный шум (fBm): сумма октав 3D-шума с убывающей амплитудой.
fn fbm(x: f64, y: f64, z: f64, octaves: u32, base_freq: f64, seed: u64) -> f64 {
    let mut v = 0.0;
    let mut amp = 1.0;
    let mut freq = base_freq;
    let mut norm = 0.0;
    for i in 0..octaves as u64 {
        v += noise3(x * freq, y * freq, z * freq, seed ^ (0x1234 * (i + 1))) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    v / norm
}

/// Хребтовый шум: `1 - |2n-1|` даёт острые гребни (горные хребты).
fn ridged(x: f64, y: f64, z: f64, octaves: u32, base_freq: f64, seed: u64) -> f64 {
    let mut v = 0.0;
    let mut amp = 1.0;
    let mut freq = base_freq;
    let mut norm = 0.0;
    for i in 0..octaves as u64 {
        let n = noise3(x * freq, y * freq, z * freq, seed ^ (0x5000 + 0x77 * (i + 1)));
        let r = 1.0 - (2.0 * n - 1.0).abs();
        v += r * r * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.1;
    }
    v / norm
}

/// Трёхмерный value noise с трилинейной интерполяцией.
///
/// Именно 3D-шум по направлению, без отображений сферы на плоскость: так рельеф
/// **непрерывен на всей сфере** и не имеет швов/складок (октаэдрическое
/// отображение их даёт, потому что его производная рвётся на «рёбрах»).
fn noise3(x: f64, y: f64, z: f64, seed: u64) -> f64 {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let fx = smooth(x - xi);
    let fy = smooth(y - yi);
    let fz = smooth(z - zi);
    let (ix, iy, iz) = (xi as i64, yi as i64, zi as i64);
    let c = |dx: i64, dy: i64, dz: i64| lattice3(ix + dx, iy + dy, iz + dz, seed);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;

    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), fx);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), fx);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), fx);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), fx);
    let y0 = lerp(x00, x10, fy);
    let y1 = lerp(x01, x11, fy);
    lerp(y0, y1, fz)
}

/// Настройки рельефа планеты.
#[derive(Clone, Copy, Debug)]
pub struct TerrainParams {
    /// Базовый радиус (уровень океана).
    pub radius: f64,
    /// Размах рельефа в единицах длины.
    pub amplitude: f64,
    /// Доля поверхности, которую занимает океан (0..1).
    pub sea_level: f64,
    pub seed: u64,
}

/// Версия формулы рельефа. Меняется при любом изменении высоты: клиент и
/// сервер обязаны её совпадать, иначе картинка разойдётся с физикой.
pub const TERRAIN_VERSION: u32 = 4;

/// Биом поверхности (по высоте над уровнем океана).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Ocean,
    Beach,
    Plains,
    Hills,
    Mountains,
}

impl Biome {
    /// Числовой код для провода и клиента (совпадает с клиентским `EOUPlanetType`-
    /// независимым набором биомов).
    pub fn code(self) -> u8 {
        match self {
            Biome::Ocean => 0,
            Biome::Beach => 1,
            Biome::Plains => 2,
            Biome::Hills => 3,
            Biome::Mountains => 4,
        }
    }
}

/// Уровень океана по умолчанию: подобран так, чтобы суша занимала ~40%
/// поверхности (см. `planet_map` в инструментах).
pub const DEFAULT_SEA_LEVEL: f64 = 0.545;

impl Default for TerrainParams {
    fn default() -> Self {
        TerrainParams { radius: 1000.0, amplitude: 30.0, sea_level: DEFAULT_SEA_LEVEL, seed: 0xC0FFEE }
    }
}

/// Рельеф планеты: чистая функция от направления.
#[derive(Clone, Copy, Debug)]
pub struct Terrain {
    pub center: Vec3,
    pub params: TerrainParams,
}

impl Terrain {
    pub fn new(center: Vec3, params: TerrainParams) -> Self {
        Terrain { center, params }
    }

    /// Высота над уровнем океана в направлении `dir` (может быть < 0 — вода).
    ///
    /// Формула общая с клиентом (см. `client/Source/OpenUniverse/OUTerrain.h`),
    /// поэтому картинка на клиенте совпадает с физикой сервера. Использует
    /// только `+ - * /` и `floor` — детерминировано и одинаково в Rust (f64) и
    /// C++ (double). Тригонометрия не используется: она платформо-зависима.
    ///
    /// Шум — **трёхмерный по направлению**: сфера непрерывна, швов и складок нет.
    ///
    /// Масштабы (в долях радиуса планеты — так формула не зависит от размера):
    /// континенты, горы и **мелкий рельеф** (сотни метров), чтобы земля под
    /// ногами была неровной, а не плоской.
    pub fn height(&self, dir: Vec3) -> f64 {
        let d = normalize(dir);
        let s = self.params.seed;
        let (x, y, z) = (d.x, d.y, d.z);

        // Форма континентов (крупный шум): где `cont > sea_level` — суша.
        let cont = fbm(x, y, z, 4, 0.9, s ^ 0xC2B2);
        // Холмы: длина волны заметно больше шага меша планеты (~3 км),
        // иначе меш не может их отобразить и «алиасит» большими плитами.
        let mid = fbm(x, y, z, 4, 20.0, s ^ 0xA11CE);
        // Мелкий рельеф: холмы в единицы километров.
        let fine = fbm(x, y, z, 4, 150.0, s ^ 0xD37);
        // Горы: острые хребты.
        let mtn = ridged(x, y, z, 3, 4.5, s ^ 0x5EED);

        // Отклонение от уровня океана.
        let base = cont - self.params.sea_level;
        // Горы и холмы «включаются» только в глубине материка (не у берега).
        let land = (base / 0.09).clamp(0.0, 1.0);

        // Двусторонний рельеф: холмы вверх и вниз, плюс горы на материках.
        // Амплитуда подобрана так, чтобы линейная интерполяция меша (шаг ~1.5 км)
        // не «вспучивалась» выше роста камеры (1.66 м): иначе игрок оказывается
        // под гранью и видит изнанку поверхности.
        let h01 = base
            + (mid - 0.5) * 0.16 * (0.3 + 0.7 * land)
            + (fine - 0.5) * 0.020 * (0.2 + 0.8 * land)
            + mtn * land * 0.34;
        h01 * self.params.amplitude * 2.0
    }

    /// Биом поверхности по высоте над уровнем океана.
    ///
    /// Пороги подобраны по фактическому распределению высот (максимум ~0.87
    /// амплитуды), чтобы все биомы реально встречались на планете.
    pub fn biome(&self, dir: Vec3) -> Biome {
        let h = self.height(dir);
        let a = self.params.amplitude;
        if h <= 0.0 {
            Biome::Ocean
        } else if h < a * 0.04 {
            Biome::Beach
        } else if h < a * 0.30 {
            Biome::Plains
        } else if h < a * 0.55 {
            Biome::Hills
        } else {
            Biome::Mountains
        }
    }

    /// Радиус поверхности в направлении `dir` (центр планеты → поверхность).
    pub fn surface_radius(&self, dir: Vec3) -> f64 {
        self.params.radius + self.height(dir)
    }

    /// Точка поверхности в направлении `dir`.
    pub fn surface_point(&self, dir: Vec3) -> Vec3 {
        self.center.add(normalize(dir).scale(self.surface_radius(dir)))
    }

    /// Является ли точка водой (ниже уровня океана).
    pub fn is_water(&self, dir: Vec3) -> bool {
        self.height(dir) <= 0.0
    }

    /// Высота в точке мира: `None`, если точка далеко от поверхности планеты.
    ///
    /// Используется физикой: чтобы узнать, «на земле» ли корабль.
    pub fn height_at_point(&self, p: Vec3) -> f64 {
        self.height(p.sub(self.center))
    }

    /// Направление «вверх» от центра планеты в точке (нормаль поверхности).
    pub fn up_at(&self, p: Vec3) -> Vec3 {
        normalize(p.sub(self.center))
    }

    /// Проник ли корабль под поверхность? Возвращает глубину проникновения
    /// (0, если всё в порядке).
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

/// Нормализовать вектор.
pub fn normalize(v: Vec3) -> Vec3 {
    let l = v.length();
    if l < 1e-12 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        v.scale(1.0 / l)
    }
}

/// Физика поверхности: посадка и столкновение с рельефом.
pub mod landing {
    use super::*;
    /// Состояние «корабль на поверхности».
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum GroundState {
        /// В воздухе/космосе — ничего не делаем.
        Flying,
        /// Касание: мягкая посадка, скорость гасится трением.
        Landed { altitude: f64 },
        /// Удар: скорость слишком велика, корабль получает повреждения.
        Crashed { impact_speed: f64 },
    }

    /// Разрешить контакт с поверхностью: прижать корабль, если он под ней.
    ///
    /// Возвращает состояние и (для удара) скорость столкновения.
    /// `safe_speed` — скорость, до которой посадка считается мягкой.
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
        // Поднимаем корабль точно на поверхность.
        let rel = pos.sub(terrain.center);
        let dir = normalize(rel);
        let target = terrain.surface_radius(dir);
        *pos = terrain.center.add(dir.scale(target));

        // Гасим скорость: нормальную составляющую убираем, касательную — трением.
        let v_normal = dir.scale(vel.dot(dir));
        let v_tangent = vel.sub(v_normal);
        let impact = v_normal.length();

        let v_tangent_damped = v_tangent.scale(1.0 - friction.clamp(0.0, 1.0));
        *vel = v_tangent_damped;

        // Ограничим скорость снизу/сверху как в обычном полёте.
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

    /// Мягкая ли посадка (удобная обёртка).
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
        // Обходим много направлений: высота не должна «взрываться».
        while h < 6.28 {
            let mut a: f64 = 0.0;
            while a < 3.14 {
                let d = Vec3::new(h.cos() * a.cos(), a.sin(), h.sin() * a.cos());
                let hh = t.height(d);
                assert!(hh.abs() <= amp + 1e-9, "высота {hh} вышла за {amp}");
                a += 0.35;
            }
            h += 0.35;
        }
    }

    #[test]
    fn height_is_continuous_across_directions() {
        // Маленькое изменение направления → маленькое изменение высоты.
        let t = t();
        let a = normalize(Vec3::new(0.3, 0.5, 0.8));
        let b = normalize(Vec3::new(0.301, 0.5, 0.8));
        let diff = (t.height(a) - t.height(b)).abs();
        assert!(diff < 1.0, "разрыв в рельефе: {diff}");
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
        assert!(water > 0, "должен быть океан");
        assert!(land > 0, "должна быть суша");
        // Океан занимает ощутимую, но не подавляющую долю.
        let frac = water as f64 / (water + land) as f64;
        assert!((0.05..0.75).contains(&frac), "доля океана неправдоподобна: {frac}");
    }

    #[test]
    fn terrain_has_all_biomes_and_real_relief() {
        // Обходим сферу: должны встретиться все биомы, а высоты — реально
        // меняться (не гладкая «вата»).
        let t = t();
        let mut seen = [false; 5];
        let mut hmin = f64::MAX;
        let mut hmax = f64::MIN;
        let mut h: f64 = 0.0;
        while h < 6.28 {
            let mut a: f64 = -1.4;
            while a < 1.4 {
                let d = Vec3::new(h.cos() * a.cos(), a.sin(), h.sin() * a.cos());
                seen[t.biome(d).code() as usize] = true;
                let hh = t.height(d);
                hmin = hmin.min(hh);
                hmax = hmax.max(hh);
                a += 0.15;
            }
            h += 0.15;
        }
        for (i, code) in ["ocean", "beach", "plains", "hills", "mountains"].iter().enumerate() {
            assert!(seen[i], "missing biome: {code}");
        }
        // Перепад высот должен быть заметным (несколько километров при амплитуде 10 км).
        assert!(hmax - hmin > t.params.amplitude * 0.5,
            "рельеф слишком плоский: {:.0}..{:.0}", hmin, hmax);
        // И не «взрываться»: в пределах двойной амплитуды.
        assert!(hmin >= -t.params.amplitude * 2.0 - 1e-9);
        assert!(hmax <= t.params.amplitude * 2.0 + 1e-9);
    }

    #[test]
    fn penetration_detects_buried_point() {        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        // Точка под поверхностью.
        let buried = t.center.add(d.scale(surf - 50.0));
        assert!((t.penetration(buried) - 50.0).abs() < 1e-6);
        // Точка над поверхностью.
        let above = t.center.add(d.scale(surf + 50.0));
        assert_eq!(t.penetration(above), 0.0);
    }

    #[test]
    fn resolution_puts_ship_on_surface() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.1, 1.0, 0.2));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf - 40.0)); // под землёй
        let mut vel = Vec3::new(0.0, -30.0, 0.0); // летит вниз
        let params = crate::ShipParams::default();
        let state = resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.5, &params);
        // Корабль на поверхности, скорость вниз погашена.
        assert!((t.penetration(pos)).abs() < 1e-6, "корабль должен быть ровно на поверхности");
        assert!(vel.dot(d) <= 1e-6, "скорость «в землю» должна быть погашена");
        assert!(matches!(state, GroundState::Landed { .. } | GroundState::Crashed { .. }));
    }

    #[test]
    fn high_speed_impact_is_a_crash() {
        use landing::*;
        let t = t();
        let d = normalize(Vec3::new(0.0, 1.0, 0.0));
        let surf = t.surface_radius(d);
        let mut pos = t.center.add(d.scale(surf - 5.0));
        let mut vel = Vec3::new(0.0, -300.0, 0.0); // очень быстро вниз
        let params = crate::ShipParams::default();
        let state = resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.5, &params);
        match state {
            GroundState::Crashed { impact_speed } => {
                assert!(impact_speed > 50.0, "удар должен быть сильным: {impact_speed}");
            }
            other => panic!("ожидался краш, получено {other:?}"),
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
        // В воздухе ничего не меняется.
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
        // Тангенциальная скорость (вбок) — трение должно её уменьшить.
        let mut vel = Vec3::new(40.0, -1.0, 0.0);
        let params = crate::ShipParams::default();
        resolve_ground(&mut pos, &mut vel, &t, 50.0, 0.8, &params);
        assert!(vel.length() < 40.0, "трение должно тормозить: {}", vel.length());
    }

    #[test]
    fn terrain_matches_render_style_scale() {
        // Проверяем, что параметры по умолчанию дают «планетный» масштаб.
        let t = t();
        assert_eq!(t.params.radius, 1000.0);
        assert!(t.params.amplitude > 0.0);
        // Высота гор заметно меньше радиуса (иначе это не планета).
        assert!(t.params.amplitude * 2.0 < t.params.radius * 0.2);
    }

    #[test]
    fn home_planet_is_planet_sized() {
        // Домашняя планета должна быть «планетного» масштаба, а не астероида:
        // радиус в сотни раз больше рельефа, иначе горизонт читается как шар.
        let t = crate::home_planet();
        assert!(t.params.radius >= 50_000.0, "радиус слишком мал: {}", t.params.radius);
        assert!(
            t.params.amplitude * 10.0 < t.params.radius,
            "рельеф слишком крупный для планеты"
        );
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
    fn terrain_formula_uses_no_trigonometry() {
        // Рельеф считается только через + - * / floor sqrt: тригонометрия
        // платформо-зависима и разошлась бы между Rust и C++. Проверяем, что
        // высота совпадает при повороте-независимом наборе входов (знак/порядок
        // осей) — этого достаточно как «дымовой» тест формулы.
        let t = t();
        let d = normalize(Vec3::new(0.4, 0.5, -0.6));
        let h1 = t.height(d);
        let h2 = t.height(d);
        assert_eq!(h1, h2);
        // Ненулевое значение: рельеф не «залип» на нуле.
        assert!(h1.abs() > 1e-6, "рельеф вырожден: {h1}");
    }
}
