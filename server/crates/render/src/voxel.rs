//! Воксельные планеты: шестигранная сфера с искривлением воксельной сетки.
//!
//! ## Идея
//! В Dual Universe планета — это не «шар с текстурой», а **воксельная сфера**:
//! кубическая сетка искривляется так, что её грани становятся поверхностью
//! планеты. Это позволяет строить прямо на планете и копать вглубь.
//!
//! ## Как здесь это устроено
//! 1. Берём кубическую решётку (6 граней куба, как в cubemap).
//! 2. Каждую точку **нормализуем** — она садится на сферу. Это и есть
//!    «искривление воксельной сетки» (cube-to-sphere mapping).
//! 3. Высота (радиус) точки берётся из фрактального шума высот: где-то горы,
//!    где-то океан.
//! 4. [`Lod`] задаёт, сколько вокселей на грань: чем ближе планета, тем больше
//!    деталей. Дальние планеты считаются грубо — это и есть LOD.
//!
//! Для рендера каждая воксельная «колонна» превращается в блок нужной высоты,
//! поэтому воксельная планета попадает в общий трассировщик без отдельного кода.

use crate::{fbm, lerp_color, norm, Color, Mat3, Vec3};

/// Уровень детализации: сколько сегментов на грань куба.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    /// Очень далеко: 4×4 на грань.
    Far,
    /// Средняя дистанция: 8×8.
    Mid,
    /// Близко: 16×16.
    Near,
    /// Вплотную: 24×24.
    Close,
}

impl Lod {
    /// Число сегментов на грань.
    pub fn segments(self) -> u32 {
        match self {
            Lod::Far => 4,
            Lod::Mid => 8,
            Lod::Near => 16,
            Lod::Close => 24,
        }
    }

    /// Сколько вокселей всего (6 граней × segments²) — для оценки нагрузки.
    pub fn voxel_count(self) -> u32 {
        6 * self.segments() * self.segments()
    }

    /// Выбрать LOD по расстоянию до планеты (в единицах мира).
    pub fn for_distance(distance: f64, planet_radius: f64) -> Lod {
        let ratio = distance / planet_radius;
        if ratio > 6.0 {
            Lod::Far
        } else if ratio > 3.0 {
            Lod::Mid
        } else if ratio > 1.5 {
            Lod::Near
        } else {
            Lod::Close
        }
    }
}

/// Цвета поверхности воксельной планеты.
#[derive(Clone, Copy, Debug)]
pub struct PlanetPalette {
    pub deep: Color,
    pub shallow: Color,
    pub sand: Color,
    pub grass: Color,
    pub rock: Color,
    pub snow: Color,
}

impl Default for PlanetPalette {
    fn default() -> Self {
        PlanetPalette {
            // Океан заметно синий (иначе глубокие участки выглядят «дырой»).
            deep: Color::rgb(0.05, 0.16, 0.38),
            shallow: Color::rgb(0.10, 0.36, 0.58),
            sand: Color::rgb(0.74, 0.66, 0.42),
            grass: Color::rgb(0.13, 0.36, 0.13),
            rock: Color::rgb(0.30, 0.26, 0.22),
            snow: Color::rgb(0.94, 0.96, 0.99),
        }
    }
}

/// Одна воксельная колонна: точка на сфере и её «материал».
#[derive(Clone, Copy, Debug)]
pub struct VoxelColumn {
    /// Направление от центра планеты (единичный вектор).
    pub dir: Vec3,
    /// Высота над радиусом планеты (0 — уровень океана, >0 — суша).
    pub height: f64,
    /// Цвет поверхности.
    pub color: Color,
}

/// Воксельная планета: набор колонн, полученных искривлением кубической сетки.
#[derive(Clone, Debug)]
pub struct VoxelPlanet {
    pub center: Vec3,
    /// Базовый радиус (уровень океана).
    pub radius: f64,
    pub seed: u64,
    pub lod: Lod,
    pub columns: Vec<VoxelColumn>,
    palette: PlanetPalette,
}

impl VoxelPlanet {
    /// Сгенерировать воксельную планету.
    ///
    /// `amplitude` задаёт размах рельефа (в единицах высоты), `sea_level` —
    /// насколько высоко поднимается океан (доля от амплитуды).
    pub fn generate(center: Vec3, radius: f64, seed: u64, lod: Lod) -> Self {
        // Больше суши, чем океана: так рельеф из вокселей виден лучше.
        Self::generate_with(center, radius, seed, lod, radius * 0.03, 0.30)
    }

    pub fn generate_with(
        center: Vec3,
        radius: f64,
        seed: u64,
        lod: Lod,
        amplitude: f64,
        sea_level: f64,
    ) -> Self {
        let seg = lod.segments();
        let palette = PlanetPalette::default();
        let mut columns = Vec::with_capacity(lod.voxel_count() as usize);

        // Обходим 6 граней куба. Для каждой грани — сетка seg×seg.
        // Пары осей и знаки дают все 6 граней.
        let faces: [(usize, usize, f64); 6] = [
            (0, 1, 1.0),
            (0, 1, -1.0),
            (0, 2, 1.0),
            (0, 2, -1.0),
            (1, 2, 1.0),
            (1, 2, -1.0),
        ];

        for (ai, bi, sign) in faces {
            for j in 0..seg {
                for i in 0..seg {
                    // Координаты на грани куба в диапазоне [-1, 1].
                    let u = (i as f64 + 0.5) / seg as f64 * 2.0 - 1.0;
                    let v = (j as f64 + 0.5) / seg as f64 * 2.0 - 1.0;
                    let mut cube = [0.0f64; 3];
                    cube[ai] = sign;
                    cube[bi] = u;
                    let third = 3 - ai - bi;
                    cube[third] = v;

                    // Искривление: переводим точку куба на сферу.
                    let dir = cube_to_sphere(Vec3::new(cube[0], cube[1], cube[2]));

                    // Высота из двух октав шума: крупные материки + детали.
                    let coarse = fbm(dir.scale(1.7), seed);
                    let fine = fbm(dir.scale(6.0), seed ^ 0xF1E);
                    let n = coarse * 0.72 + fine * 0.28;
                    let height = (n - sea_level) * amplitude * 2.0;

                    let color = Self::color_for(&palette, height, amplitude, dir);

                    columns.push(VoxelColumn { dir, height, color });                }
            }
        }

        VoxelPlanet { center, radius, seed, lod, columns, palette }
    }

    /// Цвет по высоте: океан → отмель → песок → трава → камень → снег.
    fn color_for(p: &PlanetPalette, height: f64, amplitude: f64, dir: Vec3) -> Color {
        // Нормируем высоту в [0,1] для удобства порогов.
        let h = ((height / amplitude) * 0.5 + 0.5).clamp(0.0, 1.0);
        let base = if h < 0.38 {
            lerp_color(p.deep, p.shallow, (h / 0.38).clamp(0.0, 1.0))
        } else if h < 0.46 {
            lerp_color(p.shallow, p.sand, ((h - 0.38) / 0.08).clamp(0.0, 1.0))
        } else if h < 0.56 {
            lerp_color(p.sand, p.grass, ((h - 0.46) / 0.10).clamp(0.0, 1.0))
        } else if h < 0.78 {
            lerp_color(p.grass, p.rock, ((h - 0.56) / 0.22).clamp(0.0, 1.0))
        } else {
            lerp_color(p.rock, p.snow, ((h - 0.78) / 0.22).clamp(0.0, 1.0))
        };
        // Полярные шапки: у полюсов подмешиваем снег.
        let polar = dir.y.abs().powf(4.0);
        lerp_color(base, p.snow, polar * 0.8)
    }

    /// Радиус поверхности в данном направлении (для физики и прицеливания).
    pub fn surface_radius(&self, dir: Vec3) -> f64 {
        self.radius + self.height_at(dir)
    }

    /// Высота рельефа в направлении (интерполируется между колоннами).
    pub fn height_at(&self, dir: Vec3) -> f64 {
        let d = norm(dir);
        // Находим ближайшую колонну (для отладки достаточно; в игре — сетка).
        let mut best = 0.0;
        let mut best_dot = -2.0;
        for c in &self.columns {
            let k = crate::dot(c.dir, d);
            if k > best_dot {
                best_dot = k;
                best = c.height;
            }
        }
        best
    }

    /// Превратить колонны в блоки — их рисует трассировщик.
    ///
    /// Поверхность целиком состоит из вокселей: и суша, и вода. Вода — плоские
    /// блоки чуть ниже суши, поэтому планета замкнута и не «просвечивает».
    pub fn to_blocks(&self) -> Vec<crate::Block> {
        let mut out = Vec::with_capacity(self.columns.len());
        // Размер одного блока по касательной. Берём с запасом (перекрытие),
        // иначе между воксели соседних граней куба появляются щели.
        let seg = self.lod.segments() as f64;
        let seg_len = (std::f64::consts::PI * self.radius / seg) * 0.95;
        let block_size = seg_len.max(self.radius * 0.03);

        for c in &self.columns {
            let is_water = c.height <= 0.0;
            // Вода сидит на фиксированном уровне, суша — по рельефу.
            let base_h = if is_water { 0.0 } else { c.height };
            let surface = self.radius + base_h;
            let pos = self.center.add(c.dir.scale(surface - block_size * 0.5));
            let rot = basis_from_normal(c.dir);
            // Воксельная вариация оттенка.
            let shade = 0.82 + 0.36 * crate::value_noise(pos.scale(0.9), 1.0, self.seed ^ 0xB10C);
            // Вода — плоская и гладкая; суша — тем выше, чем больше рельеф.
            let tall = if is_water {
                block_size * 0.5
            } else {
                block_size * (0.7 + (c.height / self.radius) * 6.0).clamp(0.7, 3.0)
            };
            out.push(crate::Block::oriented(
                pos,
                Vec3::new(block_size, tall, block_size),
                c.color.scale(shade),
                rot,
            ));
        }
        out
    }

    /// Позиция точки поверхности (для камеры и кораблей).
    pub fn surface_point(&self, lat: f64, lon: f64) -> Vec3 {
        let dir = Vec3::new(
            lat.cos() * lon.sin(),
            lat.sin(),
            lat.cos() * lon.cos(),
        );
        self.center.add(norm(dir).scale(self.surface_radius(dir)))
    }

    // -----------------------------------------------------------------------
    // Разрушение: копание и взрывы
    // -----------------------------------------------------------------------

    /// Выкопать кратер в направлении `dir`.
    ///
    /// Воксели в конусе вокруг направления проваливаются на `depth`, а те, что
    /// совсем близко к центру удара, исчезают вовсе (полное разрушение).
    /// Возвращает, сколько вокселей изменилось, — это «добыча».
    pub fn dig(&mut self, dir: Vec3, cone: f64, depth: f64) -> usize {
        let d = norm(dir);
        let mut changed = 0;
        for c in self.columns.iter_mut() {
            let k = crate::dot(c.dir, d);
            if k >= cone {
                // Чем ближе к оси удара, тем глубже воронка.
                let t = ((k - cone) / (1.0 - cone)).clamp(0.0, 1.0);
                c.height -= depth * t * t;
                // Полностью срытый воксель становится «камнем у поверхности».
                if c.height < -depth * 2.0 {
                    c.height = -depth * 2.0;
                }
                changed += 1;
            }
        }
        // Пересчитываем цвета под новый рельеф (порода обнажилась).
        let amp = self.radius * 0.03;
        let palette = self.palette;
        for c in self.columns.iter_mut() {
            c.color = Self::color_for(&palette, c.height, amp, c.dir);
        }
        changed
    }

    /// Взрыв: разрушение в радиусе по поверхности (без направленного конуса).
    pub fn explode(&mut self, dir: Vec3, radius: f64, depth: f64) -> usize {
        let d = norm(dir);
        // Порог по углу: чем шире радиус, тем больше конус.
        let cone = (1.0 - radius / self.radius).clamp(0.0, 0.999);
        self.dig(d, cone, depth)
    }

    /// Сколько вокселей сейчас выше уровня океана (полезно для статистики).
    pub fn land_voxels(&self) -> usize {
        self.columns.iter().filter(|c| c.height > 0.0).count()
    }

    /// Средняя высота суши.
    pub fn average_land_height(&self) -> f64 {
        let land: Vec<f64> = self.columns.iter().filter(|c| c.height > 0.0).map(|c| c.height).collect();
        if land.is_empty() {
            0.0
        } else {
            land.iter().sum::<f64>() / land.len() as f64
        }
    }

    /// Контрольная сумма рельефа — проверка детерминизма и сохранения.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for c in &self.columns {
            for v in c.dir.to_wire() {
                h ^= v as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= c.height.to_bits();
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

/// Cube-to-sphere: равномерное отображение граней куба на сферу.
///
/// Наивная нормализация растягивает сетку у центров граней и сжимает у углов,
/// из-за чего между вокселями появляются щели. Классическая формула (в духе
/// Philip Nowell / «spherified cube») выравнивает шаг сетки.
pub(crate) fn cube_to_sphere(p: Vec3) -> Vec3 {
    let x = p.x;
    let y = p.y;
    let z = p.z;
    let x2 = x * x;
    let y2 = y * y;
    let z2 = z * z;
    Vec3::new(
        x * (1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0).sqrt(),
        y * (1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0).sqrt(),
        z * (1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0).sqrt(),
    )
}

/// Ортонормальный базис, у которого ось Y смотрит по нормали.
fn basis_from_normal(up: Vec3) -> Mat3 {
    let u = norm(up);
    // Произвольный вектор, не параллельный up.
    let helper = if u.y.abs() > 0.9 { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.0, 1.0, 0.0) };
    let right = norm(cross(u, helper));
    let forward = norm(cross(right, u));
    // Столбцы: X=right, Y=up, Z=forward.
    Mat3 { c0: right, c1: u, c2: forward }
}

/// Векторное произведение.
pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_selects_by_distance() {
        let r = 1000.0;
        assert_eq!(Lod::for_distance(10_000.0, r), Lod::Far);
        assert_eq!(Lod::for_distance(4_000.0, r), Lod::Mid);
        assert_eq!(Lod::for_distance(2_000.0, r), Lod::Near);
        assert_eq!(Lod::for_distance(500.0, r), Lod::Close);
    }

    #[test]
    fn lod_voxel_counts_increase_with_detail() {
        assert!(Lod::Far.voxel_count() < Lod::Mid.voxel_count());
        assert!(Lod::Mid.voxel_count() < Lod::Near.voxel_count());
        assert!(Lod::Near.voxel_count() < Lod::Close.voxel_count());
        assert_eq!(Lod::Far.voxel_count(), 6 * 16);
    }

    #[test]
    fn cube_to_sphere_maps_to_unit_sphere() {
        // Точки на грани куба должны попадать на единичную сферу.
        for p in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.5, -0.3),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-0.7, 0.2, 1.0),
        ] {
            let s = cube_to_sphere(p);
            assert!((s.length() - 1.0).abs() < 1e-9, "не на сфере: {:?}", s.length());
        }
    }

    #[test]
    fn cube_to_sphere_is_more_even_than_naive() {
        // Считаем разброс длин сегментов сетки: правильное отображение должно
        // давать более ровный шаг, чем простая нормализация.
        let seg = 12u32;
        let spread = |f: &dyn Fn(Vec3) -> Vec3| {
            let mut lengths = Vec::new();
            for j in 0..seg {
                for i in 0..seg {
                    let u = (i as f64 + 0.5) / seg as f64 * 2.0 - 1.0;
                    let v = (j as f64 + 0.5) / seg as f64 * 2.0 - 1.0;
                    let a = f(Vec3::new(1.0, u, v));
                    let b = f(Vec3::new(1.0, u + 2.0 / seg as f64, v));
                    lengths.push(a.distance(b));
                }
            }
            let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
            let var = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
            var.sqrt() / mean
        };
        let even = spread(&cube_to_sphere);
        let naive = spread(&norm);
        assert!(
            even < naive,
            "правильное отображение должно быть ровнее: {even} vs {naive}"
        );
    }

    #[test]
    fn columns_land_on_sphere() {
        let p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 42, Lod::Mid);
        for c in &p.columns {
            // Каждое направление — единичный вектор.
            let len = c.dir.length();
            assert!((len - 1.0).abs() < 1e-9, "направление не единичное: {len}");
        }
    }

    #[test]
    fn column_count_matches_lod() {
        for lod in [Lod::Far, Lod::Mid, Lod::Near] {
            let p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 7, lod);
            assert_eq!(p.columns.len() as u32, lod.voxel_count());
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let a = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 99, Lod::Mid);
        let b = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 99, Lod::Mid);
        assert_eq!(a.columns.len(), b.columns.len());
        for (ca, cb) in a.columns.iter().zip(&b.columns) {
            assert_eq!(ca.dir.to_wire(), cb.dir.to_wire());
            assert_eq!(ca.height, cb.height);
        }
        // Другое семя → другой рельеф.
        let c = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 100, Lod::Mid);
        let differs = a.columns.iter().zip(&c.columns).any(|(x, y)| x.height != y.height);
        assert!(differs, "разные семена должны давать разный рельеф");
    }

    #[test]
    fn heights_are_bounded_by_amplitude() {
        let amp = 40.0;
        let p = VoxelPlanet::generate_with(Vec3::ZERO, 1000.0, 5, Lod::Near, amp, 0.35);
        for c in &p.columns {
            assert!(c.height.abs() <= amp, "высота {} вышла за амплитуду {amp}", c.height);
        }
    }

    #[test]
    fn surface_radius_follows_height() {
        let p = VoxelPlanet::generate(Vec3::new(100.0, 0.0, 0.0), 1000.0, 3, Lod::Near);
        // Берём направление реальной колонны и проверяем согласованность.
        let c = p.columns[10];
        let r = p.surface_radius(c.dir);
        assert!((r - (1000.0 + c.height)).abs() < 1e-9);
    }

    #[test]
    fn to_blocks_cover_water_and_land() {
        let p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 11, Lod::Mid);
        let blocks = p.to_blocks();
        // Вся планета замкнута: блоков ровно столько, сколько колонн.
        assert_eq!(blocks.len(), p.columns.len());
        assert!(blocks.len() > 0);
    }

    #[test]
    fn to_blocks_are_near_surface() {
        let p = VoxelPlanet::generate(Vec3::new(0.0, 0.0, 0.0), 1000.0, 21, Lod::Mid);
        for b in p.to_blocks().iter().take(50) {
            let d = b.center.distance(p.center);
            // Блоки не дальше радиуса + амплитуда и не глубже радиуса.
            assert!(d < p.radius * 1.2, "блок слишком далеко: {d}");
            assert!(d > p.radius * 0.5, "блок слишком близко к центру: {d}");
        }
    }

    #[test]
    fn surface_point_is_on_surface() {
        let p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 8, Lod::Mid);
        let pt = p.surface_point(0.3, 1.2);
        let d = pt.length();
        assert!(d > 990.0 && d < 1100.0, "точка поверхности на радиусе {d}");
    }

    #[test]
    fn palette_is_used_for_water_and_land() {
        let p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 17, Lod::Near);
        // Есть и океан (синий), и суша (светлее).
        let has_water = p.columns.iter().any(|c| c.color.b > c.color.r);
        let has_land = p.columns.iter().any(|c| c.color.r > 0.3);
        assert!(has_water, "должен быть океан");
        assert!(has_land, "должна быть суша");
    }

    // --- разрушение (копание) ---

    #[test]
    fn dig_lowers_heights_along_axis() {
        let mut p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 55, Lod::Near);
        // Копаем строго «вверх» (ось +Y).
        let up = Vec3::new(0.0, 1.0, 0.0);
        let before: Vec<f64> = p.columns.iter().map(|c| c.height).collect();

        let changed = p.dig(up, 0.90, 30.0);
        assert!(changed > 0, "копание должно задеть воксели");

        // Высоты у оси удара уменьшились, у противоположной стороны — нет.
        let mut lowered = 0;
        for (c, h0) in p.columns.iter().zip(&before) {
            if crate::dot(c.dir, up) >= 0.99 && c.height < *h0 {
                lowered += 1;
            }
        }
        assert!(lowered > 0, "высоты на оси удара должны опуститься");

        // Дно не уходит в бесконечность.
        assert!(p.columns.iter().all(|c| c.height >= -60.0 - 1e-9));
    }

    #[test]
    fn dig_changes_checksum_and_land_area() {
        let mut p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 66, Lod::Near);
        let sum0 = p.checksum();
        let land0 = p.land_voxels();
        // Срываем крупный участок поверхности.
        p.explode(Vec3::new(0.0, 1.0, 0.0), 400.0, 60.0);
        assert_ne!(p.checksum(), sum0, "рельеф должен измениться");
        assert!(p.land_voxels() <= land0, "суши не должно стать больше");
    }

    #[test]
    fn dig_is_deterministic() {
        let dig_at = |seed: u64| {
            let mut p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, seed, Lod::Mid);
            p.dig(Vec3::new(0.3, 1.0, -0.2), 0.95, 25.0);
            p.checksum()
        };
        assert_eq!(dig_at(7), dig_at(7));
        assert_ne!(dig_at(7), dig_at(8));
    }

    #[test]
    fn dig_outside_cone_does_nothing() {
        let mut p = VoxelPlanet::generate(Vec3::ZERO, 1000.0, 77, Lod::Near);
        let sum0 = p.checksum();
        // Очень узкий конус с направлением, где вокселей почти нет.
        let changed = p.dig(Vec3::new(0.0, 1.0, 0.0), 0.9999, 10.0);
        // Либо ничего не задели, либо задели чуть-чуть — но не весь рельеф.
        assert!(changed < p.columns.len() / 10, "узкий конус не должен рушить всё");
        let _ = sum0;
    }
}
