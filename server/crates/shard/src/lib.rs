//! # ou-shard
//!
//! Шардирование единого мира Open Universe.
//!
//! ## Идея
//! Мир — это один непрерывный шард, но физически он делится на **узлы**
//! (nodes). Каждый узел владеет набором **зон** (звёздных систем). Узел
//! симулирует только свои зоны и не тратит ресурсы на чужие.
//!
//! ## Что здесь есть
//! * [`ZoneId`] — идентификатор зоны (звёздной системы).
//! * [`ShardMap`] — детерминированное отображение «зона → узел»
//!   (consistent hashing по кольцу виртуальных узлов). При добавлении или
//!   удалении узла переезжает только малая доля зон.
//! * [`ShardRouter`] — маршрутизация запроса к владельцу зоны и определение
//!   момента **миграции**: игрок, пересекающий границу зон, передаётся
//!   соседнему узлу.
//!
//! ## Детерминизм
//! Отображение зон на узлы — чистая функция от (имя зоны, список узлов), без
//! случайности во время выполнения. Все узлы независимо приходят к одному и
//! тому же распределению, поэтому миграция не требует «арбитра».

use ou_protocol::Vec3;
use ou_sim::StarSystem;
use std::collections::BTreeMap;

/// Идентификатор зоны (обычно = id звёздной системы).
pub type ZoneId = u32;
/// Идентификатор узла-владельца.
pub type NodeId = u32;

/// Хеш строки (FNV-1a + сильный финализатор) — детерминированная функция,
/// одинаковая везде. Финализатор нужен, чтобы похожие строки (`node-1-vnode-0`,
/// `node-1-vnode-1`, …) давали хорошо разнесённые позиции на кольце.
fn hash64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // SplitMix64 finalizer: лавинное перемешивание битов.
    let mut z = h;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Сколько виртуальных точек на кольце создаёт каждый узел.
/// Больше точек — равномернее распределение, но дороже поиск.
const VNODES_PER_NODE: u32 = 64;

/// Отображение «зона → узел» через кольцо consistent hashing.
#[derive(Clone, Debug, Default)]
pub struct ShardMap {
    /// (позиция на кольце) → узел.
    ring: BTreeMap<u64, NodeId>,
    /// Узлы, участвующие в кольце.
    nodes: Vec<NodeId>,
}

impl ShardMap {
    pub fn new() -> Self {
        ShardMap { ring: BTreeMap::new(), nodes: Vec::new() }
    }

    /// Сформировать карту из списка узлов.
    pub fn with_nodes(nodes: &[NodeId]) -> Self {
        let mut m = ShardMap::new();
        for n in nodes {
            m.add_node(*n);
        }
        m
    }

    /// Добавить узел (идемпотентно).
    pub fn add_node(&mut self, node: NodeId) {
        if self.nodes.contains(&node) {
            return;
        }
        self.nodes.push(node);
        self.nodes.sort_unstable();
        for i in 0..VNODES_PER_NODE {
            let key = hash64(&format!("node-{node}-vnode-{i}"));
            // Коллизии крайне редки; при совпадении точку просто перезаписываем.
            self.ring.insert(key, node);
        }
    }

    /// Удалить узел (идемпотентно).
    pub fn remove_node(&mut self, node: NodeId) {
        if !self.nodes.contains(&node) {
            return;
        }
        self.nodes.retain(|n| *n != node);
        self.ring.retain(|_, v| *v != node);
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Владелец зоны. Детерминирован: одинаковый вход → одинаковый ответ.
    pub fn owner(&self, zone: ZoneId, system_name: &str) -> Option<NodeId> {
        if self.ring.is_empty() {
            return None;
        }
        let key = hash64(&format!("zone-{zone}-{system_name}"));
        // Первая точка по часовой стрелке; если вышли за кольцо — заворачиваем.
        self.ring
            .range(key..)
            .next()
            .or_else(|| self.ring.iter().next())
            .map(|(_, node)| *node)
    }
}

/// Результат маршрутизации игрока.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Routing {
    /// Игрок на своём узле — ничего делать не нужно.
    Local,
    /// Игрок перешёл в зону другого узла — нужна миграция на `to`.
    Migrate { to: NodeId },
    /// Мир ещё не сформирован.
    Unassigned,
}

/// Маршрутизатор единого шарда.
#[derive(Clone, Debug)]
pub struct ShardRouter {
    pub map: ShardMap,
    /// Имя зоны по её id (для детерминированного хеша).
    pub zone_names: BTreeMap<ZoneId, String>,
    /// Центр каждой зоны в мировых координатах.
    pub zone_centers: BTreeMap<ZoneId, Vec3>,
    /// Радиус зоны (переход за него = смена зоны).
    pub zone_radius: f64,
}

impl ShardRouter {
    pub fn new(zone_radius: f64) -> Self {
        ShardRouter {
            map: ShardMap::new(),
            zone_names: BTreeMap::new(),
            zone_centers: BTreeMap::new(),
            zone_radius,
        }
    }

    /// Зарегистрировать звёздную систему как зону.
    pub fn register_system(&mut self, sys: &StarSystem) {
        self.zone_names.insert(sys.id, sys.name.clone());
        self.zone_centers.insert(sys.id, sys.pos);
    }

    /// Зона, которой принадлежит точка (ближайший центр). Детерминировано:
    /// при равенстве расстояний выбирается меньший id.
    pub fn zone_at(&self, pos: Vec3) -> Option<ZoneId> {
        let mut best: Option<(f64, ZoneId)> = None;
        for (id, center) in &self.zone_centers {
            let d = pos.distance(*center);
            match best {
                None => best = Some((d, *id)),
                Some((bd, bid)) => {
                    if d < bd || (d == bd && *id < bid) {
                        best = Some((d, *id));
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// Владелец зоны.
    pub fn owner_of(&self, zone: ZoneId) -> Option<NodeId> {
        let name = self.zone_names.get(&zone)?;
        self.map.owner(zone, name)
    }

    /// Куда должен идти игрок, находящийся в точке `pos` на узле `current`.
    pub fn route(&self, pos: Vec3, current: NodeId) -> Routing {
        if self.map.is_empty() {
            return Routing::Unassigned;
        }
        match self.zone_at(pos).and_then(|z| self.owner_of(z)) {
            None => Routing::Unassigned,
            Some(node) if node == current => Routing::Local,
            Some(node) => Routing::Migrate { to: node },
        }
    }
}

/// Снимок состояния игрока для передачи на другой узел при миграции.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransferState {
    pub player_id: u32,
    pub entity_id: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub last_input_seq: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ou_sim::generate_system;

    #[test]
    fn owner_is_deterministic() {
        let m = ShardMap::with_nodes(&[1, 2, 3]);
        for zone in 0..50u32 {
            let a = m.owner(zone, "Alpha-1");
            let b = m.owner(zone, "Alpha-1");
            assert_eq!(a, b, "владелец зоны должен быть стабилен");
            assert!(a.is_some());
        }
    }

    #[test]
    fn distribution_is_reasonably_even() {
        let m = ShardMap::with_nodes(&[1, 2, 3, 4]);
        let mut counts: BTreeMap<NodeId, u32> = BTreeMap::new();
        for zone in 0..2000u32 {
            if let Some(n) = m.owner(zone, &format!("sys-{zone}")) {
                *counts.entry(n).or_default() += 1;
            }
        }
        assert_eq!(counts.len(), 4);
        let total: u32 = counts.values().sum();
        assert_eq!(total, 2000);
        // Ни один узел не должен получить больше 45% и меньше 5%.
        for (node, c) in &counts {
            let share = *c as f64 / total as f64;
            assert!(share > 0.05 && share < 0.45, "узел {node}: доля {share}");
        }
    }

    #[test]
    fn consistent_hashing_moves_few_zones_on_rebalance() {
        let before = ShardMap::with_nodes(&[1, 2, 3]);
        let owners_before: Vec<_> = (0..1000u32).map(|z| before.owner(z, &format!("s{z}"))).collect();

        let mut after = before.clone();
        after.add_node(4);
        let owners_after: Vec<_> = (0..1000u32).map(|z| after.owner(z, &format!("s{z}"))).collect();

        let moved = owners_before
            .iter()
            .zip(&owners_after)
            .filter(|(a, b)| a != b)
            .count();
        // При добавлении 4-го узла должно переехать около 1/4 зон, но не больше 40%.
        assert!(moved > 100, "переехало слишком мало: {moved}");
        assert!(moved < 400, "переехало слишком много: {moved}");
    }

    #[test]
    fn removing_node_reassigns_its_zones() {
        let mut m = ShardMap::with_nodes(&[1, 2]);
        let owned_by_2: Vec<u32> = (0..500u32)
            .filter(|z| m.owner(*z, &format!("s{z}")) == Some(2))
            .collect();
        m.remove_node(2);
        assert_eq!(m.nodes(), &[1]);
        for z in owned_by_2 {
            assert_eq!(m.owner(z, &format!("s{z}")), Some(1), "зона {z} должна уйти узлу 1");
        }
    }

    #[test]
    fn empty_map_has_no_owner() {
        let m = ShardMap::new();
        assert!(m.is_empty());
        assert_eq!(m.owner(1, "x"), None);
    }

    #[test]
    fn router_finds_zone_by_nearest_center() {
        let mut r = ShardRouter::new(10_000.0);
        r.register_system(&generate_system(1, 1, Vec3::new(0.0, 0.0, 0.0)));
        r.register_system(&generate_system(2, 2, Vec3::new(100_000.0, 0.0, 0.0)));

        assert_eq!(r.zone_at(Vec3::new(100.0, 0.0, 0.0)), Some(1));
        assert_eq!(r.zone_at(Vec3::new(99_900.0, 0.0, 0.0)), Some(2));
    }

    #[test]
    fn router_routes_local_and_migration() {
        let mut r = ShardRouter::new(10_000.0);
        r.map = ShardMap::with_nodes(&[1, 2, 3]); // узлы обязаны существовать
        r.register_system(&generate_system(1, 1, Vec3::new(0.0, 0.0, 0.0)));
        r.register_system(&generate_system(2, 2, Vec3::new(100_000.0, 0.0, 0.0)));
        let a = r.owner_of(1).unwrap();
        let b = r.owner_of(2).unwrap();

        // Точка в центре зоны 1 → локально для её владельца.
        assert_eq!(r.route(Vec3::ZERO, a), Routing::Local);
        // Точка в центре зоны 2 → локально для владельца зоны 2.
        assert_eq!(r.route(Vec3::new(100_000.0, 0.0, 0.0), b), Routing::Local);
        // Если зоны у разных узлов — перелёт даёт миграцию.
        if a != b {
            assert_eq!(
                r.route(Vec3::new(100_000.0, 0.0, 0.0), a),
                Routing::Migrate { to: b }
            );
        }
    }

    #[test]
    fn owner_of_is_none_without_system() {
        let mut r = ShardRouter::new(1000.0);
        r.map = ShardMap::with_nodes(&[1]);
        // Система не зарегистрирована → имени нет → владельца нет.
        assert_eq!(r.owner_of(99), None);
    }

    #[test]
    fn unassigned_when_no_nodes() {
        let mut r = ShardRouter::new(1000.0);
        r.register_system(&generate_system(1, 1, Vec3::ZERO));
        assert_eq!(r.route(Vec3::ZERO, 1), Routing::Unassigned);
    }
}
