//! # ou-build
//!
//! Воксельный редактор конструкций Open Universe: **ставить, удалять, красить**.
//!
//! ## Что редактируется
//! Игрок строит из вокселей в сетке конструкции (корабль, станция, база).
//! Сетка разрежённая: хранятся только занятые ячейки (`BTreeMap`), поэтому
//! большая конструкция не требует памяти под весь объём.
//!
//! ## Детерминизм
//! Порядок обхода — `BTreeMap` по целочисленным координатам. Одинаковая
//! последовательность правок даёт одинаковую конструкцию и одинаковую
//! контрольную сумму — это нужно для сохранения и проверки на сервере.
//!
//! ## Целостность
//! [`VoxelGrid::validate`] проверяет связность: все воксели обязаны быть
//! соединены с «ядром» конструкции. Отвалившаяся деталь — не корабль, а мусор,
//! поэтому с сервера она снимается.

use ou_protocol::Vec3;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Целочисленная координата вокселя в сетке конструкции.
pub type Coord = (i32, i32, i32);

/// Цвет вокселя (RGB, 0..255). Хранится целым — детерминированно.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const HULL: Color = Color(186, 196, 214);
    pub const DARK: Color = Color(107, 117, 140);
    pub const ACCENT: Color = Color(41, 158, 184);
    pub const GLASS: Color = Color(31, 51, 87);
    pub const ENGINE: Color = Color(89, 242, 255);
    pub const RED: Color = Color(200, 60, 60);
    pub const GREEN: Color = Color(60, 200, 90);

    /// Разложить в линейный 0..1 для рендера.
    pub fn to_f64(self) -> (f64, f64, f64) {
        (self.0 as f64 / 255.0, self.1 as f64 / 255.0, self.2 as f64 / 255.0)
    }

    /// Из линейного 0..1.
    pub fn from_f64(r: f64, g: f64, b: f64) -> Color {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color(q(r), q(g), q(b))
    }
}

/// Ошибка редактирования.
#[derive(Clone, Debug, PartialEq)]
pub enum BuildError {
    /// Ячейка уже занята (ставить некуда).
    Occupied(Coord),
    /// Ячейки нет (нечего удалять или красить).
    Empty(Coord),
    /// Выход за границы допустимого объёма конструкции.
    OutOfBounds(Coord),
    /// Превышен лимит вокселей у игрока.
    TooManyVoxels { limit: usize, current: usize },
    /// Ячейка оторвана от ядра (нарушение целостности).
    Disconnected(Coord),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Occupied(c) => write!(f, "ячейка {:?} уже занята", c),
            BuildError::Empty(c) => write!(f, "в ячейке {:?} ничего нет", c),
            BuildError::OutOfBounds(c) => write!(f, "ячейка {:?} вне допустимого объёма", c),
            BuildError::TooManyVoxels { limit, current } => {
                write!(f, "слишком много вокселей: {current} при лимите {limit}")
            }
            BuildError::Disconnected(c) => write!(f, "воксель {:?} оторван от ядра", c),
        }
    }
}

impl std::error::Error for BuildError {}

/// Лимиты редактора (защита от «строительства вселенной»).
#[derive(Clone, Copy, Debug)]
pub struct BuildLimits {
    /// Максимум вокселей в одной конструкции.
    pub max_voxels: usize,
    /// Максимальный размер стороны конструкции (в вокселях).
    pub max_extent: i32,
    /// Проверять ли связность после каждой правки.
    pub check_integrity: bool,
}

impl Default for BuildLimits {
    fn default() -> Self {
        BuildLimits { max_voxels: 20_000, max_extent: 128, check_integrity: true }
    }
}

/// Воксельная конструкция.
#[derive(Clone, Debug)]
pub struct VoxelGrid {
    voxels: BTreeMap<Coord, Color>,
    /// Ячейка «ядра» — обязательная точка, с которой всё должно быть связано.
    core: Coord,
    pub limits: BuildLimits,
}

impl VoxelGrid {
    /// Новая конструкция с ядром в начале координат.
    pub fn new() -> Self {
        let mut g = VoxelGrid {
            voxels: BTreeMap::new(),
            core: (0, 0, 0),
            limits: BuildLimits::default(),
        };
        // Ядро — первый воксель.
        g.voxels.insert((0, 0, 0), Color::ACCENT);
        g
    }

    pub fn with_limits(limits: BuildLimits) -> Self {
        let mut g = VoxelGrid::new();
        g.limits = limits;
        g
    }

    pub fn core(&self) -> Coord {
        self.core
    }

    pub fn len(&self) -> usize {
        self.voxels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voxels.is_empty()
    }

    pub fn get(&self, c: Coord) -> Option<Color> {
        self.voxels.get(&c).copied()
    }

    /// Все занятые ячейки в детерминированном порядке.
    pub fn iter(&self) -> impl Iterator<Item = (Coord, Color)> + '_ {
        self.voxels.iter().map(|(k, v)| (*k, *v))
    }

    /// Центр масс конструкции (для камеры и физики).
    pub fn centroid(&self) -> Vec3 {
        if self.voxels.is_empty() {
            return Vec3::ZERO;
        }
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        for c in self.voxels.keys() {
            x += c.0 as f64;
            y += c.1 as f64;
            z += c.2 as f64;
        }
        let n = self.voxels.len() as f64;
        Vec3::new(x / n, y / n, z / n)
    }

    /// Габариты (ширина, высота, глубина) в вокселях.
    pub fn extent(&self) -> (i32, i32, i32) {
        if self.voxels.is_empty() {
            return (0, 0, 0);
        }
        let (mut min, mut max) = ((i32::MAX, i32::MAX, i32::MAX), (i32::MIN, i32::MIN, i32::MIN));
        for c in self.voxels.keys() {
            min.0 = min.0.min(c.0);
            min.1 = min.1.min(c.1);
            min.2 = min.2.min(c.2);
            max.0 = max.0.max(c.0);
            max.1 = max.1.max(c.1);
            max.2 = max.2.max(c.2);
        }
        (max.0 - min.0 + 1, max.1 - min.1 + 1, max.2 - min.2 + 1)
    }

    fn in_bounds(&self, c: Coord) -> bool {
        let e = self.limits.max_extent;
        c.0.abs() <= e && c.1.abs() <= e && c.2.abs() <= e
    }

    /// Поставить воксель.
    pub fn place(&mut self, c: Coord, color: Color) -> Result<(), BuildError> {
        if !self.in_bounds(c) {
            return Err(BuildError::OutOfBounds(c));
        }
        if self.voxels.contains_key(&c) {
            return Err(BuildError::Occupied(c));
        }
        if self.voxels.len() >= self.limits.max_voxels {
            return Err(BuildError::TooManyVoxels {
                limit: self.limits.max_voxels,
                current: self.voxels.len(),
            });
        }
        // Деталь должна примыкать к существующей (иначе нельзя ставить в пустоте).
        if !self.touches_existing(c) {
            return Err(BuildError::Disconnected(c));
        }
        self.voxels.insert(c, color);
        if self.limits.check_integrity {
            if let Err(e) = self.validate() {
                // Откат: не оставляем мир в неконсистентном виде.
                self.voxels.remove(&c);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Удалить воксель.
    pub fn remove(&mut self, c: Coord) -> Result<Color, BuildError> {
        let color = self.voxels.get(&c).copied().ok_or(BuildError::Empty(c))?;
        if c == self.core {
            // Ядро нельзя снести — иначе конструкция рассыпается целиком.
            return Err(BuildError::Empty(c));
        }
        self.voxels.remove(&c);
        Ok(color)
    }

    /// Перекрасить воксель.
    pub fn paint(&mut self, c: Coord, color: Color) -> Result<(), BuildError> {
        match self.voxels.get_mut(&c) {
            Some(slot) => {
                *slot = color;
                Ok(())
            }
            None => Err(BuildError::Empty(c)),
        }
    }

    /// Примыкает ли ячейка к уже существующему вокселю (6-связность).
    fn touches_existing(&self, c: Coord) -> bool {
        self.neighbors(c).any(|n| self.voxels.contains_key(&n))
    }

    /// Соседи по 6 направлениям.
    fn neighbors(&self, c: Coord) -> impl Iterator<Item = Coord> {
        const D: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        D.into_iter().map(move |d| (c.0 + d.0, c.1 + d.1, c.2 + d.2))
    }

    /// Проверить связность: все воксели должны быть достижимы от ядра.
    pub fn validate(&self) -> Result<(), BuildError> {
        if self.voxels.is_empty() {
            return Err(BuildError::Empty(self.core));
        }
        let mut seen: BTreeSet<Coord> = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.core);
        seen.insert(self.core);

        while let Some(c) = queue.pop_front() {
            for n in self.neighbors(c) {
                if self.voxels.contains_key(&n) && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }

        if seen.len() != self.voxels.len() {
            // Найти первый оторванный воксель (детерминированно).
            for c in self.voxels.keys() {
                if !seen.contains(c) {
                    return Err(BuildError::Disconnected(*c));
                }
            }
        }
        Ok(())
    }

    /// Снять все оторванные воксели (уборка «мусора»).
    ///
    /// Возвращает число удалённых.
    pub fn prune_disconnected(&mut self) -> usize {
        if self.voxels.is_empty() {
            return 0;
        }
        let mut seen: BTreeSet<Coord> = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.core);
        seen.insert(self.core);
        while let Some(c) = queue.pop_front() {
            for n in self.neighbors(c) {
                if self.voxels.contains_key(&n) && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        let orphans: Vec<Coord> = self
            .voxels
            .keys()
            .filter(|c| !seen.contains(c))
            .copied()
            .collect();
        for c in &orphans {
            self.voxels.remove(c);
        }
        orphans.len()
    }

    /// Контрольная сумма конструкции (для сохранения и проверки).
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for (c, col) in &self.voxels {
            for v in [c.0 as i64, c.1 as i64, c.2 as i64] {
                h ^= v as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= ((col.0 as u64) << 16) | ((col.1 as u64) << 8) | col.2 as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    /// Масса конструкции (вокселей × масса одного) — для физики.
    pub fn mass(&self, per_voxel: f64) -> f64 {
        self.voxels.len() as f64 * per_voxel
    }
}

impl Default for VoxelGrid {
    fn default() -> Self {
        VoxelGrid::new()
    }
}

/// Готовая форма корабля — стартовый шаблон для игрока (blueprint).
pub fn ship_blueprint() -> VoxelGrid {
    let mut g = VoxelGrid::new();
    let mut place = |c: Coord, col: Color| {
        let _ = g.place(c, col);
    };
    // Корпус вдоль +Z.
    for z in 1..=6 {
        for x in -1..=1 {
            for y in 0..=1 {
                place((x, y, z), if (x + y) % 2 == 0 { Color::HULL } else { Color::DARK });
            }
        }
    }
    // Крылья.
    for x in [-2, 2] {
        for z in 2..=4 {
            place((x, 0, z), Color::ACCENT);
        }
    }
    // Кабина.
    place((0, 2, 3), Color::GLASS);
    // Двигатели.
    place((0, 0, 7), Color::ENGINE);
    place((0, 1, 7), Color::ENGINE);
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_has_core() {
        let g = VoxelGrid::new();
        assert_eq!(g.len(), 1);
        assert_eq!(g.get((0, 0, 0)), Some(Color::ACCENT));
    }

    #[test]
    fn place_adjacent_voxel() {
        let mut g = VoxelGrid::new();
        assert!(g.place((1, 0, 0), Color::HULL).is_ok());
        assert_eq!(g.len(), 2);
        assert_eq!(g.get((1, 0, 0)), Some(Color::HULL));
    }

    #[test]
    fn cannot_place_in_empty_space() {
        let mut g = VoxelGrid::new();
        // Далеко от ядра — нельзя.
        let err = g.place((5, 0, 0), Color::HULL).unwrap_err();
        assert_eq!(err, BuildError::Disconnected((5, 0, 0)));
    }

    #[test]
    fn cannot_place_on_occupied() {
        let mut g = VoxelGrid::new();
        g.place((1, 0, 0), Color::HULL).unwrap();
        assert_eq!(g.place((1, 0, 0), Color::DARK).unwrap_err(), BuildError::Occupied((1, 0, 0)));
    }

    #[test]
    fn remove_and_paint() {
        let mut g = VoxelGrid::new();
        g.place((1, 0, 0), Color::HULL).unwrap();
        g.paint((1, 0, 0), Color::RED).unwrap();
        assert_eq!(g.get((1, 0, 0)), Some(Color::RED));
        assert_eq!(g.remove((1, 0, 0)).unwrap(), Color::RED);
        assert_eq!(g.len(), 1);
        assert_eq!(g.remove((1, 0, 0)).unwrap_err(), BuildError::Empty((1, 0, 0)));
    }

    #[test]
    fn cannot_remove_core() {
        let mut g = VoxelGrid::new();
        assert!(g.remove((0, 0, 0)).is_err(), "ядро нельзя снести");
    }

    #[test]
    fn out_of_bounds_is_rejected() {
        let mut g = VoxelGrid::with_limits(BuildLimits { max_extent: 4, ..Default::default() });
        // Строим цепочку до границы, затем пробуем выйти за неё.
        for x in 1..=4 {
            g.place((x, 0, 0), Color::HULL).unwrap();
        }
        // x = 5 — уже вне границы.
        let err = g.place((5, 0, 0), Color::HULL).unwrap_err();
        assert_eq!(err, BuildError::OutOfBounds((5, 0, 0)));
    }

    #[test]
    fn voxel_limit_is_enforced() {
        let mut g = VoxelGrid::with_limits(BuildLimits {
            max_voxels: 3,
            check_integrity: false,
            ..Default::default()
        });
        g.place((1, 0, 0), Color::HULL).unwrap();
        g.place((2, 0, 0), Color::HULL).unwrap();
        let err = g.place((3, 0, 0), Color::HULL).unwrap_err();
        assert!(matches!(err, BuildError::TooManyVoxels { .. }));
    }

    #[test]
    fn integrity_detects_orphans_and_prunes() {
        let mut g = VoxelGrid::new();
        // Строим мостик.
        for x in 1..=3 {
            g.place((x, 0, 0), Color::HULL).unwrap();
        }
        // Вручную добавляем оторванный воксель (имитируем повреждение).
        g.voxels.insert((10, 10, 10), Color::RED);
        assert!(g.validate().is_err(), "оторванный воксель должен ломать целостность");
        let removed = g.prune_disconnected();
        assert_eq!(removed, 1);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn extent_and_centroid() {
        let mut g = VoxelGrid::new();
        g.place((1, 0, 0), Color::HULL).unwrap();
        g.place((-1, 0, 0), Color::HULL).unwrap();
        let (w, h, d) = g.extent();
        assert_eq!((w, h, d), (3, 1, 1));
        let c = g.centroid();
        assert!((c.x - 0.0).abs() < 1e-9, "центр масс по x = {}", c.x);
    }

    #[test]
    fn same_edits_give_same_checksum() {
        let build = || {
            let mut g = VoxelGrid::new();
            g.place((1, 0, 0), Color::HULL).unwrap();
            g.place((0, 1, 0), Color::DARK).unwrap();
            g.paint((0, 1, 0), Color::ACCENT).unwrap();
            g.checksum()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn different_edits_give_different_checksum() {
        let mut a = VoxelGrid::new();
        a.place((1, 0, 0), Color::HULL).unwrap();
        let mut b = VoxelGrid::new();
        b.place((0, 1, 0), Color::HULL).unwrap();
        assert_ne!(a.checksum(), b.checksum());
    }

    #[test]
    fn blueprint_is_connected_and_valid() {
        let g = ship_blueprint();
        assert!(g.len() > 10, "шаблон корабля должен быть не пустым");
        assert!(g.validate().is_ok(), "шаблон обязан быть связным");
        let (w, h, d) = g.extent();
        assert!(d >= w, "корабль вытянут вдоль оси Z");
        let _ = h;
    }

    #[test]
    fn mass_scales_with_voxels() {
        let g = ship_blueprint();
        let m = g.mass(2.0);
        assert!((m - g.len() as f64 * 2.0).abs() < 1e-9);
    }

    #[test]
    fn placing_far_chain_then_removing_middle_splits_validly() {
        let mut g = VoxelGrid::new();
        // Цепочка из 5 вокселей.
        for x in 1..=5 {
            g.place((x, 0, 0), Color::HULL).unwrap();
        }
        assert!(g.validate().is_ok());
        // Сносим середину — цепочка распадается на две части: правая оторвана.
        g.remove((3, 0, 0)).unwrap();
        // Целостность нарушена: правый отрезок (4,5) не связан с ядром.
        assert!(g.validate().is_err(), "отвалившаяся часть должна нарушать целостность");
        let pruned = g.prune_disconnected();
        assert_eq!(pruned, 2, "должны сняться воксели 4 и 5");
        assert!(g.validate().is_ok());
    }
}
