//! # ou-spatial
//!
//! Пространственный индекс для interest management: равномерная сетка
//! (uniform grid). Заменяет линейный перебор всех сущностей на обход только
//! тех ячеек, что пересекают зону интереса.
//!
//! ## Детерминизм
//! Все коллекции — `BTreeMap`/`BTreeSet`, поэтому результат запроса всегда
//! отсортирован по [`NetId`] и не зависит от порядка вставки. Это обязательное
//! условие для воспроизводимой симуляции.

use ou_protocol::{NetId, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// Координаты ячейки сетки.
pub type Cell = (i32, i32, i32);

/// Равномерная пространственная сетка.
#[derive(Clone, Debug)]
pub struct SpatialGrid {
    /// Размер ячейки по каждой оси.
    cell: f64,
    /// Обратный размер — умножение вместо деления (и быстрее, и детерминированнее).
    inv_cell: f64,
    cells: BTreeMap<Cell, BTreeSet<NetId>>,
    positions: BTreeMap<NetId, Vec3>,
}

impl SpatialGrid {
    /// Создать сетку с размером ячейки `cell` (должен быть больше 0).
    pub fn new(cell: f64) -> Self {
        assert!(cell > 0.0, "размер ячейки должен быть положительным");
        SpatialGrid {
            cell,
            inv_cell: 1.0 / cell,
            cells: BTreeMap::new(),
            positions: BTreeMap::new(),
        }
    }

    pub fn cell_size(&self) -> f64 {
        self.cell
    }

    /// Число объектов в индексе.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    #[inline]
    fn cell_of(&self, p: Vec3) -> Cell {
        // floor — детерминированная операция IEEE-754.
        (
            (p.x * self.inv_cell).floor() as i32,
            (p.y * self.inv_cell).floor() as i32,
            (p.z * self.inv_cell).floor() as i32,
        )
    }

    /// Добавить объект. Повторная вставка того же `id` перемещает его.
    pub fn insert(&mut self, id: NetId, pos: Vec3) {
        if let Some(old) = self.positions.get(&id).copied() {
            if old == pos {
                return;
            }
            let old_cell = self.cell_of(old);
            if let Some(set) = self.cells.get_mut(&old_cell) {
                set.remove(&id);
                if set.is_empty() {
                    self.cells.remove(&old_cell);
                }
            }
        }
        self.positions.insert(id, pos);
        self.cells.entry(self.cell_of(pos)).or_default().insert(id);
    }

    /// Переместить объект (синоним `insert`).
    pub fn update(&mut self, id: NetId, pos: Vec3) {
        self.insert(id, pos);
    }

    pub fn remove(&mut self, id: NetId) -> bool {
        match self.positions.remove(&id) {
            Some(p) => {
                let c = self.cell_of(p);
                if let Some(set) = self.cells.get_mut(&c) {
                    set.remove(&id);
                    if set.is_empty() {
                        self.cells.remove(&c);
                    }
                }
                true
            }
            None => false,
        }
    }

    pub fn position(&self, id: NetId) -> Option<Vec3> {
        self.positions.get(&id).copied()
    }

    /// Объекты в радиусе `radius` от `center`, отсортированные по `NetId`.
    ///
    /// Обход затрагивает только ячейки, покрывающие куб вокруг сферы запроса.
    pub fn query_radius(&self, center: Vec3, radius: f64) -> Vec<NetId> {
        let r2 = radius * radius;
        let min = Vec3::new(center.x - radius, center.y - radius, center.z - radius);
        let max = Vec3::new(center.x + radius, center.y + radius, center.z + radius);
        let (x0, y0, z0) = self.cell_of(min);
        let (x1, y1, z1) = self.cell_of(max);

        let mut out = BTreeSet::new();
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                for cz in z0..=z1 {
                    if let Some(set) = self.cells.get(&(cx, cy, cz)) {
                        for id in set {
                            if let Some(p) = self.positions.get(id) {
                                // Сравнение квадратов расстояний — без sqrt.
                                let d = p.sub(center);
                                if d.x * d.x + d.y * d.y + d.z * d.z <= r2 {
                                    out.insert(*id);
                                }
                            }
                        }
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// Число занятых ячеек (для диагностики/тестов).
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Сколько объектов проверяет `query_radius` без пространственного отсечения —
    /// то есть объём работы наивного O(n) метода. Полезно для оценки выигрыша.
    pub fn saturated(&self) -> usize {
        self.positions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[NetId]) -> Vec<NetId> {
        v.to_vec()
    }

    #[test]
    fn insert_and_query_finds_neighbours() {
        let mut g = SpatialGrid::new(100.0);
        g.insert(1, Vec3::new(0.0, 0.0, 0.0));
        g.insert(2, Vec3::new(50.0, 0.0, 0.0));
        g.insert(3, Vec3::new(500.0, 0.0, 0.0));
        assert_eq!(g.len(), 3);

        let near = g.query_radius(Vec3::ZERO, 120.0);
        assert_eq!(ids(&near), vec![1, 2]);

        let all = g.query_radius(Vec3::ZERO, 1000.0);
        assert_eq!(ids(&all), vec![1, 2, 3]);
    }

    #[test]
    fn query_is_sorted_and_deterministic() {
        let mut g = SpatialGrid::new(64.0);
        for id in [9u32, 3, 7, 1, 5] {
            g.insert(id, Vec3::new(id as f64, 0.0, 0.0));
        }
        let a = g.query_radius(Vec3::ZERO, 1000.0);
        let b = g.query_radius(Vec3::ZERO, 1000.0);
        assert_eq!(a, vec![1, 3, 5, 7, 9]);
        assert_eq!(a, b);
    }

    #[test]
    fn update_moves_between_cells() {
        let mut g = SpatialGrid::new(10.0);
        g.insert(1, Vec3::ZERO);
        assert_eq!(g.query_radius(Vec3::ZERO, 5.0), vec![1]);
        g.update(1, Vec3::new(1000.0, 0.0, 0.0));
        assert!(g.query_radius(Vec3::ZERO, 5.0).is_empty());
        assert_eq!(g.query_radius(Vec3::new(1000.0, 0.0, 0.0), 5.0), vec![1]);
        assert_eq!(g.cell_count(), 1, "старая ячейка должна быть очищена");
    }

    #[test]
    fn remove_clears_cell() {
        let mut g = SpatialGrid::new(10.0);
        g.insert(1, Vec3::ZERO);
        assert!(g.remove(1));
        assert!(!g.remove(1));
        assert!(g.is_empty());
        assert_eq!(g.cell_count(), 0);
    }

    #[test]
    fn radius_boundary_is_inclusive() {
        let mut g = SpatialGrid::new(10.0);
        g.insert(1, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(g.query_radius(Vec3::ZERO, 10.0), vec![1]);
        assert!(g.query_radius(Vec3::ZERO, 9.999).is_empty());
    }

    #[test]
    fn matches_brute_force_on_random_layout() {
        // Пространственный индекс обязан давать ровно тот же результат, что и
        // наивный перебор — это проверка корректности отсечения по ячейкам.
        let mut g = SpatialGrid::new(80.0);
        let mut pts: Vec<(NetId, Vec3)> = Vec::new();
        let mut h: u64 = 12345;
        let mut next = || {
            h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((h >> 33) as f64) / (u32::MAX as f64) * 4000.0 - 2000.0
        };
        for id in 0..500u32 {
            let p = Vec3::new(next(), next(), next());
            pts.push((id, p));
            g.insert(id, p);
        }
        let centers = [
            Vec3::ZERO,
            Vec3::new(1000.0, -500.0, 250.0),
            Vec3::new(-1500.0, 1500.0, -1000.0),
        ];
        for c in centers {
            for r in [0.0, 50.0, 200.0, 750.0, 3000.0] {
                let mut expect: Vec<NetId> = pts
                    .iter()
                    .filter(|(_, p)| p.distance(c) <= r)
                    .map(|(id, _)| *id)
                    .collect();
                expect.sort_unstable();
                assert_eq!(g.query_radius(c, r), expect, "center={c:?} r={r}");
            }
        }
    }
}
