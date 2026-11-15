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

/// Воксель: цвет и размер (сторона в единицах: 1 — малый, 2 — средний,
/// 3 — большой). Разные размеры стыкуются по граням, поэтому сторона хранится
/// целой и координата задаётся минимальным углом куба в единичной решётке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voxel {
    pub color: Color,
    /// Сторона куба в единицах (1..=3).
    pub size: u8,
}

impl Voxel {
    pub fn new(color: Color, size: u8) -> Self {
        Voxel { color, size: size.clamp(1, 3) }
    }
}

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

/// Ядро конструкции: размер (сторона в вокселях) и назначение.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreSpec {
    /// Сторона куба постройки в вокселях (16/32/64).
    pub size: i32,
    /// Назначение: 0 — статичная база (на планете), 1 — корабль, 2 — орбитальная.
    pub kind: u8,
}

impl Default for CoreSpec {
    fn default() -> Self {
        CoreSpec { size: 16, kind: 0 }
    }
}

impl CoreSpec {
    /// Допустимый размер ядра — только 16, 32 или 64.
    pub fn is_valid_size(size: i32) -> bool {
        matches!(size, 16 | 32 | 64)
    }

    /// Полурадиус (в вокселях) от центра ядра до границы куба.
    pub fn half(&self) -> i32 {
        self.size / 2
    }

    /// Имя назначения (для интерфейса/логов).
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            1 => "корабль",
            2 => "орбитальная база",
            _ => "статичная база",
        }
    }
}

/// Воксельная конструкция.
#[derive(Clone, Debug)]
pub struct VoxelGrid {
    /// Воксели: ключ — минимальный угол куба в единичной решётке, значение —
    /// цвет и размер стороны (1/2/3). Разрешены разные размеры: они стыкуются
    /// по граням, как кубики разного калибра (как в voxel-играх).
    voxels: BTreeMap<Coord, Voxel>,
    /// Ячейка «ядра» — обязательная точка, с которой всё должно быть связано.
    core: Coord,
    /// Основана ли постройка игроком. Пока `false`, первый поставленный блок
    /// переносит ядро на себя (как в DU); после — нужна связность с ядром.
    founded: bool,
    /// Размер и назначение ядра (куб, в котором можно строить).
    spec: CoreSpec,
    /// Размер вокселя-ядра (сторона, 1..3): ядро — тоже воксель.
    core_voxel_size: u8,
    pub limits: BuildLimits,
}

impl VoxelGrid {
    /// Новая конструкция с ядром в начале координат.
    pub fn new() -> Self {
        let mut g = VoxelGrid {
            voxels: BTreeMap::new(),
            core: (0, 0, 0),
            founded: false,
            spec: CoreSpec::default(),
            core_voxel_size: 1,
            limits: BuildLimits::default(),
        };
        // Ядро — первый воксель (малый размер по умолчанию).
        g.voxels.insert((0, 0, 0), Voxel::new(Color::ACCENT, 1));
        g
    }

    /// Размер и назначение ядра.
    pub fn spec(&self) -> CoreSpec {
        self.spec
    }

    /// Размер вокселя-ядра (сторона в единицах).
    pub fn core_voxel_size(&self) -> u8 {
        self.core_voxel_size
    }

    /// Установить ядро: задаёт размер/назначение и переносит центр сетки.
    ///
    /// Ядро — воксель выбранного размера (`voxel_size` 1/2/3), к его граням
    /// пристыковываются воксели игрока (в т.ч. другого размера).
    pub fn set_core(&mut self, coord: Coord, spec: CoreSpec, voxel_size: u8) {
        self.core = coord;
        self.spec = spec;
        self.core_voxel_size = voxel_size.clamp(1, 3);
        self.founded = true;
        // Ядро само занимает свой куб (центр постройки).
        self.voxels.clear();
        self.voxels.insert(coord, Voxel::new(Color::ACCENT, self.core_voxel_size));
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

    /// Цвет вокселя, накрывающего единичную ячейку `c`.
    pub fn get(&self, c: Coord) -> Option<Color> {
        self.voxel_at_cell(c)
            .and_then(|corner| self.voxels.get(&corner))
            .map(|v| v.color)
    }

    /// Все воксели (мин. угол, цвет, сторона) в детерминированном порядке.
    pub fn iter(&self) -> impl Iterator<Item = (Coord, Color, u8)> + '_ {
        self.voxels.iter().map(|(k, v)| (*k, v.color, v.size))
    }

    /// Ячейки (единичные), которые занимает воксель со сторонами `size` от угла `c`.
    fn voxel_cells(c: Coord, size: u8) -> Vec<Coord> {
        let s = size.max(1) as i32;
        let mut out = Vec::with_capacity((s * s * s) as usize);
        for dx in 0..s {
            for dy in 0..s {
                for dz in 0..s {
                    out.push((c.0 + dx, c.1 + dy, c.2 + dz));
                }
            }
        }
        out
    }

    /// Карта занятости: единичная ячейка → мин. угол вокселя-владельца.
    fn occupancy(&self) -> BTreeMap<Coord, Coord> {
        let mut m = BTreeMap::new();
        for (c, v) in &self.voxels {
            for cell in Self::voxel_cells(*c, v.size) {
                m.insert(cell, *c);
            }
        }
        m
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
        // Границы — куб ядра (постройка не выходит за размер ядра), но не больше
        // общего лимита конструкции.
        let e = self.limits.max_extent;
        let h = self.spec.half().min(e);
        let (dx, dy, dz) = (c.0 - self.core.0, c.1 - self.core.1, c.2 - self.core.2);
        dx.abs() <= h && dy.abs() <= h && dz.abs() <= h
    }

    /// Ограничивающий куб сетки ядра (мин/макс по каждой оси) — для отрисовки
    /// призрачной сетки и проверок.
    pub fn core_bounds(&self) -> (Coord, Coord) {
        let h = self.spec.half();
        let c = self.core;
        (
            (c.0 - h, c.1 - h, c.2 - h),
            (c.0 + h, c.1 + h, c.2 + h),
        )
    }

    /// Поставить воксель размера `size` (1/2/3) минимальным углом в `c`.
    pub fn place_sized(&mut self, c: Coord, color: Color, size: u8) -> Result<(), BuildError> {
        let size = size.clamp(1, 3);
        // Все единичные ячейки нового куба — в границах и не заняты.
        let occ = self.occupancy();
        for cell in Self::voxel_cells(c, size) {
            if !self.in_bounds(cell) {
                return Err(BuildError::OutOfBounds(cell));
            }
            if occ.contains_key(&cell) {
                return Err(BuildError::Occupied(cell));
            }
        }
        if self.voxels.len() >= self.limits.max_voxels {
            return Err(BuildError::TooManyVoxels {
                limit: self.limits.max_voxels,
                current: self.voxels.len(),
            });
        }
        // Примыкание по граням: хотя бы одна ячейка нового куба касается
        // существующего вокселя (общая грань). Первый воксель игрока основывает
        // постройку (ядро переезжает на него).
        if !self.touches_occupied(&occ, c, size) {
            if !self.founded && self.voxels.len() <= 1 {
                self.voxels.clear();
                self.core = c;
                self.core_voxel_size = size;
                self.voxels.insert(c, Voxel::new(color, size));
                self.founded = true;
                return Ok(());
            }
            return Err(BuildError::Disconnected(c));
        }
        self.founded = true;
        self.voxels.insert(c, Voxel::new(color, size));
        if self.limits.check_integrity {
            if let Err(e) = self.validate() {
                self.voxels.remove(&c);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Поставить воксель (малый размер 1) — совместимость.
    pub fn place(&mut self, c: Coord, color: Color) -> Result<(), BuildError> {
        self.place_sized(c, color, 1)
    }

    /// Есть ли у куба `[c, c+size)` общая грань с занятой ячейкой.
    fn touches_occupied(&self, occ: &BTreeMap<Coord, Coord>, c: Coord, size: u8) -> bool {
        // Проверяем шесть сторон куба: достаточно одной соседней занятой ячейки
        // за пределами куба (общая грань — стыковка разных размеров допускается).
        let s = size.max(1) as i32;
        for cell in Self::voxel_cells(c, size) {
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let n = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                // Сосед вне куба (внутри куба — часть самого вокселя).
                let inside = n.0 >= c.0 && n.0 < c.0 + s
                    && n.1 >= c.1 && n.1 < c.1 + s
                    && n.2 >= c.2 && n.2 < c.2 + s;
                if !inside && occ.contains_key(&n) {
                    return true;
                }
            }
        }
        false
    }

    /// Найти воксель, накрывающий единичную ячейку `cell` (мин. угол), если есть.
    pub fn voxel_at_cell(&self, cell: Coord) -> Option<Coord> {
        self.voxels
            .iter()
            .find(|(c, v)| {
                Self::voxel_cells(**c, v.size).iter().any(|x| *x == cell)
            })
            .map(|(c, _)| *c)
    }

    /// Удалить воксель по единичной ячейке (по любому накрывающему кубу).
    pub fn remove(&mut self, c: Coord) -> Result<Color, BuildError> {
        let corner = self
            .voxel_at_cell(c)
            .ok_or(BuildError::Empty(c))?;
        if corner == self.core {
            // Ядро нельзя снести — иначе конструкция рассыпается целиком.
            return Err(BuildError::Empty(c));
        }
        let v = self.voxels.remove(&corner).unwrap();
        Ok(v.color)
    }

    /// Перекрасить воксель по единичной ячейке.
    pub fn paint(&mut self, c: Coord, color: Color) -> Result<(), BuildError> {
        let corner = self.voxel_at_cell(c).ok_or(BuildError::Empty(c))?;
        if let Some(v) = self.voxels.get_mut(&corner) {
            v.color = color;
            return Ok(());
        }
        Err(BuildError::Empty(c))
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
    ///
    /// Связность — по граням: два вокселя соединены, если их кубы касаются
    /// (любые размеры). Ищем компоненту от ядра через карту занятости.
    pub fn validate(&self) -> Result<(), BuildError> {
        if self.voxels.is_empty() {
            return Err(BuildError::Empty(self.core));
        }
        let seen = self.connected_from_core();
        if seen.len() != self.voxels.len() {
            for c in self.voxels.keys() {
                if !seen.contains(c) {
                    return Err(BuildError::Disconnected(*c));
                }
            }
        }
        Ok(())
    }

    /// Множество мин. углов вокселей, соединённых с ядром по граням.
    fn connected_from_core(&self) -> BTreeSet<Coord> {
        let occ = self.occupancy();
        let mut seen: BTreeSet<Coord> = BTreeSet::new();
        let mut queue: VecDeque<Coord> = VecDeque::new();
        // Владелец ядра — сам воксель ядра.
        let core_owner = self.voxel_at_cell(self.core).unwrap_or(self.core);
        if !self.voxels.contains_key(&core_owner) {
            return seen;
        }
        queue.push_back(core_owner);
        seen.insert(core_owner);
        while let Some(corner) = queue.pop_front() {
            let size = self.voxels.get(&corner).map(|v| v.size).unwrap_or(1);
            // Соседи по граням: для каждой ячейки куба — внешний сосед, его
            // владелец — смежный воксель.
            for cell in Self::voxel_cells(corner, size) {
                for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                    let n = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    if let Some(owner) = occ.get(&n) {
                        if *owner != corner && seen.insert(*owner) {
                            queue.push_back(*owner);
                        }
                    }
                }
            }
        }
        seen
    }

    /// Снять все оторванные воксели (уборка «мусора»).
    ///
    /// Возвращает число удалённых.
    pub fn prune_disconnected(&mut self) -> usize {
        if self.voxels.is_empty() {
            return 0;
        }
        let seen = self.connected_from_core();
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
        for (c, v) in &self.voxels {
            for k in [c.0 as i64, c.1 as i64, c.2 as i64] {
                h ^= k as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= ((v.color.0 as u64) << 16) | ((v.color.1 as u64) << 8) | v.color.2 as u64;
            h = h.wrapping_mul(0x100000001b3);
            h ^= v.size as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    /// Масса конструкции: сумма объёмов вокселей × масса одного малого вокселя.
    /// Большие воксели тяжелее пропорционально объёму — это нужно для физики.
    pub fn mass(&self, per_voxel: f64) -> f64 {
        let vol: u64 = self
            .voxels
            .values()
            .map(|v| (v.size as u64).pow(3))
            .sum();
        vol as f64 * per_voxel
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

// ---------------------------------------------------------------------------
// Библиотека шаблонов (blueprints) для редактора в игре
// ---------------------------------------------------------------------------

/// Готовый шаблон постройки, который игрок размещает в один клик.
///
/// Шаблон — чистая функция: он выдаёт список вокселей (координата + цвет)
/// **относительно якоря**. Сервер штампует их в конструкцию игрока авторитетно,
/// поэтому клиент не может «подделать» форму: он лишь выбирает шаблон и точку.
#[derive(Clone, Copy)]
pub struct Blueprint {
    /// Стабильный id (0 зарезервирован под «нет»).
    pub id: u32,
    /// Имя для интерфейса.
    pub name: &'static str,
    /// Короткое описание.
    pub description: &'static str,
    /// Функция, выдающая воксели относительно якоря (0,0,0).
    pub build: fn() -> Vec<(Coord, Color)>,
}

/// Все шаблоны библиотеки (стабильный порядок и id).
pub const BLUEPRINTS: &[Blueprint] = &[
    Blueprint { id: 1, name: "Корабль", description: "Стартовый корабль (корпус, крылья, двигатели)", build: bp_ship },
    Blueprint { id: 2, name: "Куб 5×5×5", description: "Сплошной куб из блоков", build: bp_cube },
    Blueprint { id: 3, name: "Стена 10×4", description: "Ровная стена из блоков", build: bp_wall },
    Blueprint { id: 4, name: "Башня 3×8", description: "Колонна-башня 3×3 на 8 вверх", build: bp_tower },
    Blueprint { id: 5, name: "Платформа 12×12", description: "Ровная площадка в один слой", build: bp_platform },
    Blueprint { id: 6, name: "Рамка 7×7", description: "Пустая рамка-контур", build: bp_frame },
];

/// Найти шаблон по id.
pub fn blueprint_by_id(id: u32) -> Option<&'static Blueprint> {
    BLUEPRINTS.iter().find(|b| b.id == id)
}

/// Собрать список вокселей шаблона по id (относительно якоря).
pub fn blueprint_voxels(id: u32) -> Option<Vec<(Coord, Color)>> {
    blueprint_by_id(id).map(|b| (b.build)())
}

fn bp_ship() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for z in 0..=6 {
        for x in -1..=1 {
            for y in 0..=1 {
                out.push(((x, y, z), if (x + y) % 2 == 0 { Color::HULL } else { Color::DARK }));
            }
        }
    }
    for x in [-2, 2] {
        for z in 2..=4 {
            out.push(((x, 0, z), Color::ACCENT));
        }
    }
    out.push(((0, 2, 3), Color::GLASS));
    out.push(((0, 0, 7), Color::ENGINE));
    out.push(((0, 1, 7), Color::ENGINE));
    out
}

fn bp_cube() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for x in 0..5 {
        for y in 0..5 {
            for z in 0..5 {
                let edge = x == 0 || x == 4 || y == 0 || y == 4 || z == 0 || z == 4;
                out.push(((x, y, z), if edge { Color::HULL } else { Color::DARK }));
            }
        }
    }
    out
}

fn bp_wall() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for x in 0..10 {
        for y in 0..4 {
            let c = if y == 0 || y == 3 { Color::HULL } else { Color::DARK };
            out.push(((x, y, 0), c));
        }
    }
    out
}

fn bp_tower() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for y in 0..8 {
        for x in 0..3 {
            for z in 0..3 {
                let shell = x == 0 || x == 2 || z == 0 || z == 2;
                out.push(((x, y, z), if shell { Color::HULL } else { Color::DARK }));
            }
        }
    }
    out
}

fn bp_platform() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for x in 0..12 {
        for z in 0..12 {
            let border = x == 0 || x == 11 || z == 0 || z == 11;
            out.push(((x, 0, z), if border { Color::ACCENT } else { Color::HULL }));
        }
    }
    out
}

fn bp_frame() -> Vec<(Coord, Color)> {
    let mut out = Vec::new();
    for x in 0..7 {
        for y in 0..7 {
            let border = x == 0 || x == 6 || y == 0 || y == 6;
            if border {
                out.push(((x, y, 0), Color::ACCENT));
            }
        }
    }
    out
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
        // Первый блок (в пределах куба ядра) основывает постройку: ядро переезжает.
        assert!(g.place((5, 0, 0), Color::HULL).is_ok());
        assert_eq!(g.core(), (5, 0, 0), "ядро переехало на первый блок");
        assert_eq!(g.len(), 1);
        // Второй блок обязан примыкать: далёкий (но в границах куба) — отклонён.
        let err = g.place((5 + 5, 0, 0), Color::HULL).unwrap_err();
        assert_eq!(err, BuildError::Disconnected((10, 0, 0)));
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
        g.voxels.insert((10, 10, 10), Voxel::new(Color::RED, 1));
        assert!(g.validate().is_err(), "оторванный воксель должен ломать целостность");
        let removed = g.prune_disconnected();
        assert_eq!(removed, 1);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn different_voxel_sizes_connect_face_to_face() {
        let mut g = VoxelGrid::new();
        // Ядро — большой воксель (3) в начале координат.
        g.set_core((0, 0, 0), CoreSpec { size: 16, kind: 0 }, 3);
        assert_eq!(g.len(), 1);
        assert_eq!(g.get((0, 0, 0)), Some(Color::ACCENT));
        // Малый воксель к грани большого: угол (3,0,0) — вплотную по X.
        assert!(g.place_sized((3, 0, 0), Color::HULL, 1).is_ok(), "малый встаёт к большому по грани");
        // Средний — тоже стыкуется (грань с ядром 3×3×3 сверху: угол (0,3,0)).
        assert!(g.place_sized((0, 3, 0), Color::DARK, 2).is_ok(), "средний стыкуется по грани");
        // Связность цела: оба вокселя соединены с ядром.
        assert!(g.validate().is_ok());
        // Постановка в пустоте (не касается) — отклоняется.
        assert!(g.place_sized((10, 10, 10), Color::RED, 1).is_err(), "в пустоте нельзя");
    }

    #[test]
    fn big_voxel_occupies_whole_cube() {
        let mut g = VoxelGrid::new();
        // Средний воксель (2) занимает 8 единичных ячеек: вторая постановка в
        // накрытую ячейку отклоняется как «занято».
        assert!(g.place_sized((1, 0, 0), Color::HULL, 2).is_ok());
        assert_eq!(g.get((1, 0, 0)), Some(Color::HULL));
        assert_eq!(g.get((2, 1, 1)), Some(Color::HULL), "ячейки куба принадлежат вокселю");
        // Ячейка (2,0,0) занята тем же вокселем — повторная постановка отклонена.
        assert!(g.place_sized((2, 0, 0), Color::RED, 1).is_err());
    }

    #[test]
    fn extent_and_centroid() {        let mut g = VoxelGrid::new();
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

    #[test]
    fn blueprint_library_has_templates() {
        assert!(BLUEPRINTS.len() >= 5, "библиотека шаблонов непуста");
        for (i, b) in BLUEPRINTS.iter().enumerate() {
            assert!(b.id > 0);
            assert!(!b.name.is_empty());
            assert!(!b.description.is_empty());
            let v = (b.build)();
            assert!(!v.is_empty(), "шаблон {} пуст", b.id);
            for (j, other) in BLUEPRINTS.iter().enumerate() {
                if i != j {
                    assert_ne!(b.id, other.id, "id шаблонов уникальны");
                }
            }
        }
        assert!(blueprint_by_id(1).is_some());
        assert!(blueprint_by_id(999).is_none());
    }

    #[test]
    fn blueprint_voxels_fit_limits() {
        let lim = BuildLimits::default();
        for b in BLUEPRINTS {
            let v = (b.build)();
            assert!(v.len() <= lim.max_voxels, "шаблон {} превышает лимит вокселей", b.id);
            for (c, _) in &v {
                assert!(
                    c.0.abs() <= lim.max_extent && c.1.abs() <= lim.max_extent && c.2.abs() <= lim.max_extent,
                    "шаблон {} выходит за границы",
                    b.id
                );
            }
        }
    }
}
