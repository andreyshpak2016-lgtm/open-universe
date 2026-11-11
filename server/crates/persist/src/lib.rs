//! # ou-persist
//!
//! Персистентность мира Open Universe: чтобы единый шард переживал перезапуск
//! сервера.
//!
//! ## Формат
//! Снимок мира — **бинарный, версионированный, с контрольной суммой**.
//! Пишется атомарно: сначала во временный файл, затем `rename`. Так падение
//! процесса в середине записи не оставит повреждённый снимок.
//!
//! ## Зачем свой формат, а не сразу SQL
//! Единый шард пишет очень часто и большими объёмами. Быстрый локальный снимок
//! закрывает перезапуск уже сейчас, а трейт [`Store`] позволяет позже подключить
//! PostgreSQL/объектное хранилище без изменения кода симуляции.
//!
//! ## Детерминизм
//! Порядок сущностей задаётся `BTreeMap` в мире, поэтому побайтовый снимок
//! одинаков для одинакового состояния. Это позволяет сверять снимки суммой.

use ou_protocol::{NetId, Vec3};
use ou_sim::World;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Магия файла: "OUWS" (Open Universe World Snapshot).
pub const MAGIC: [u8; 4] = *b"OUWS";
/// Текущая версия формата снимка.
pub const FORMAT_VERSION: u16 = 1;

/// Ошибки персистентности.
#[derive(Debug)]
pub enum PersistError {
    Io(io::Error),
    /// Файл не является снимком Open Universe.
    BadMagic,
    /// Версия формата не поддерживается этим билдом.
    UnsupportedVersion(u16),
    /// Размер данных не совпал с ожидаемым (обрезанный файл).
    Truncated { expected: usize, got: usize },
    /// Контрольная сумма не сошлась — файл повреждён.
    ChecksumMismatch { expected: u64, got: u64 },
    /// Снимок описывает больше сущностей, чем допустимо (защита от мусора).
    TooManyEntities(usize),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::Io(e) => write!(f, "ввод/вывод: {e}"),
            PersistError::BadMagic => write!(f, "это не снимок Open Universe"),
            PersistError::UnsupportedVersion(v) => write!(f, "версия формата {v} не поддерживается"),
            PersistError::Truncated { expected, got } => {
                write!(f, "файл обрезан: ожидалось {expected} байт, есть {got}")
            }
            PersistError::ChecksumMismatch { expected, got } => {
                write!(f, "контрольная сумма не сошлась: ожидалось {expected:#x}, получено {got:#x}")
            }
            PersistError::TooManyEntities(n) => write!(f, "слишком много сущностей: {n}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<io::Error> for PersistError {
    fn from(e: io::Error) -> Self {
        PersistError::Io(e)
    }
}

/// Защита от повреждённого заголовка: разумный предел числа сущностей.
const MAX_ENTITIES: usize = 5_000_000;

/// Состояние одной сущности в снимке.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRecord {
    pub id: NetId,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub last_input_seq: u32,
}

/// Снимок мира целиком.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    /// Версия формата.
    pub version: u16,
    /// Тик симуляции на момент снимка.
    pub tick: u64,
    /// Сущности в детерминированном порядке.
    pub entities: Vec<EntityRecord>,
}

impl WorldSnapshot {
    /// Снять снимок с живого мира.
    pub fn capture(world: &World) -> Self {
        let entities = world
            .entities
            .iter()
            .map(|(id, e)| EntityRecord {
                id: *id,
                pos: e.state.pos,
                vel: e.state.vel,
                yaw: e.state.yaw,
                pitch: e.state.pitch,
                last_input_seq: e.last_input_seq,
            })
            .collect();
        WorldSnapshot {
            version: FORMAT_VERSION,
            tick: world.tick,
            entities,
        }
    }

    /// Восстановить мир из снимка.
    pub fn restore(&self) -> World {
        let mut world = World::new();
        world.tick = self.tick;
        for r in &self.entities {
            world.entities.insert(
                r.id,
                ou_sim::Entity {
                    state: ou_protocol::EntityState {
                        id: r.id,
                        pos: r.pos,
                        vel: r.vel,
                        yaw: r.yaw,
                        pitch: r.pitch,
                    },
                    last_input_seq: r.last_input_seq,
                    terrain: None,
                    ground: ou_sim::landing::GroundState::Flying,
                },
            );
        }
        world
    }

    /// Сериализовать в байты.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(16 + self.entities.len() * 44);
        // Заголовок.
        body.extend_from_slice(&MAGIC);
        body.extend_from_slice(&self.version.to_le_bytes());
        body.extend_from_slice(&self.tick.to_le_bytes());
        body.extend_from_slice(&(self.entities.len() as u32).to_le_bytes());
        // Тела.
        for e in &self.entities {
            body.extend_from_slice(&e.id.to_le_bytes());
            for c in e.pos.to_wire() {
                body.extend_from_slice(&c.to_le_bytes());
            }
            for c in e.vel.to_wire() {
                body.extend_from_slice(&c.to_le_bytes());
            }
            body.extend_from_slice(&e.yaw.to_le_bytes());
            body.extend_from_slice(&e.pitch.to_le_bytes());
            body.extend_from_slice(&e.last_input_seq.to_le_bytes());
        }
        // Контрольная сумма всего тела.
        let sum = fnv1a(&body);
        let mut out = body;
        out.extend_from_slice(&sum.to_le_bytes());
        out
    }

    /// Разобрать из байтов с полной валидацией.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, PersistError> {
        const TRAILER: usize = 8; // u64 суммa
        if buf.len() < 4 + 2 + 8 + 4 + TRAILER {
            return Err(PersistError::Truncated { expected: 18 + TRAILER, got: buf.len() });
        }
        if buf[0..4] != MAGIC {
            return Err(PersistError::BadMagic);
        }
        // Проверяем контрольную сумму до разбора.
        let (body, sum_bytes) = buf.split_at(buf.len() - TRAILER);
        let expected = u64::from_le_bytes(sum_bytes.try_into().unwrap());
        let got = fnv1a(body);
        if expected != got {
            return Err(PersistError::ChecksumMismatch { expected, got });
        }

        let version = u16::from_le_bytes([body[4], body[5]]);
        if version != FORMAT_VERSION {
            return Err(PersistError::UnsupportedVersion(version));
        }
        let tick = u64::from_le_bytes(body[6..14].try_into().unwrap());
        let count = u32::from_le_bytes(body[14..18].try_into().unwrap()) as usize;
        if count > MAX_ENTITIES {
            return Err(PersistError::TooManyEntities(count));
        }

        const REC: usize = 4 + 12 + 12 + 4 + 4 + 4;
        let need = 18 + count * REC;
        if body.len() < need {
            return Err(PersistError::Truncated { expected: need, got: body.len() });
        }

        let mut entities = Vec::with_capacity(count);
        let mut off = 18;
        for _ in 0..count {
            let id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            off += 4;
            let mut pos = [0i32; 3];
            for c in pos.iter_mut() {
                *c = i32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
            }
            let mut vel = [0i32; 3];
            for c in vel.iter_mut() {
                *c = i32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
            }
            let yaw = f32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            off += 4;
            let pitch = f32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            off += 4;
            let last_input_seq = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            off += 4;
            entities.push(EntityRecord {
                id,
                pos: Vec3::from_wire(pos),
                vel: Vec3::from_wire(vel),
                yaw,
                pitch,
                last_input_seq,
            });
        }
        Ok(WorldSnapshot { version, tick, entities })
    }
}

/// FNV-1a по байтам.
fn fnv1a(buf: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in buf {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Хранилище мира. Реализации: файл (сейчас), SQL/S3 (позже).
pub trait Store {
    /// Сохранить снимок (атомарно).
    fn save(&mut self, snap: &WorldSnapshot) -> Result<(), PersistError>;
    /// Загрузить последний снимок, если он есть.
    fn load(&self) -> Result<Option<WorldSnapshot>, PersistError>;
}

/// Файловое хранилище: один каталог, один слот снимка + предыдущий (backup).
#[derive(Clone, Debug)]
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    /// Открыть (создать при необходимости) каталог хранилища.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PersistError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        Ok(FileStore { dir })
    }

    fn current_path(&self) -> PathBuf {
        self.dir.join("world.snapshot")
    }

    fn backup_path(&self) -> PathBuf {
        self.dir.join("world.snapshot.prev")
    }

    fn tmp_path(&self) -> PathBuf {
        self.dir.join("world.snapshot.tmp")
    }

    /// Записать байты атомарно: tmp → fsync → rename, старый файл в backup.
    fn atomic_write(&self, bytes: &[u8]) -> Result<(), PersistError> {
        {
            let mut f = std::fs::File::create(self.tmp_path())?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        let cur = self.current_path();
        if cur.exists() {
            // Сохраняем предыдущий снимок как резерв.
            std::fs::copy(&cur, self.backup_path())?;
        }
        // rename атомарен в пределах одной файловой системы.
        std::fs::rename(self.tmp_path(), &cur)?;
        Ok(())
    }
}

impl Store for FileStore {
    fn save(&mut self, snap: &WorldSnapshot) -> Result<(), PersistError> {
        self.atomic_write(&snap.to_bytes())
    }

    fn load(&self) -> Result<Option<WorldSnapshot>, PersistError> {
        let cur = self.current_path();
        if !cur.exists() {
            return Ok(None);
        }
        let mut buf = Vec::new();
        std::fs::File::open(&cur)?.read_to_end(&mut buf)?;
        match WorldSnapshot::from_bytes(&buf) {
            Ok(s) => Ok(Some(s)),
            Err(e) => {
                // Основной файл повреждён — пробуем резервную копию.
                let bak = self.backup_path();
                if bak.exists() {
                    let mut b2 = Vec::new();
                    std::fs::File::open(&bak)?.read_to_end(&mut b2)?;
                    if let Ok(s) = WorldSnapshot::from_bytes(&b2) {
                        return Ok(Some(s));
                    }
                }
                Err(e)
            }
        }
    }
}

/// Хранилище в памяти — для тестов и отладки.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    pub last: Option<WorldSnapshot>,
    pub saves: u64,
}

impl Store for MemoryStore {
    fn save(&mut self, snap: &WorldSnapshot) -> Result<(), PersistError> {
        self.last = Some(snap.clone());
        self.saves += 1;
        Ok(())
    }
    fn load(&self) -> Result<Option<WorldSnapshot>, PersistError> {
        Ok(self.last.clone())
    }
}

/// Загрузка снимка с автопроверкой версии и построением мира.
pub fn load_world(store: &impl Store) -> Result<World, PersistError> {
    match store.load()? {
        Some(snap) => Ok(snap.restore()),
        None => Ok(World::new()),
    }
}

/// Сохранить мир в хранилище.
pub fn save_world(store: &mut impl Store, world: &World) -> Result<u64, PersistError> {
    let snap = WorldSnapshot::capture(world);
    let bytes = snap.to_bytes();
    let size = bytes.len() as u64;
    store.save(&snap)?;
    Ok(size)
}

/// Дополнительно: экспорт в простой JSON (для отладки и внешних инструментов).
pub fn to_json(snap: &WorldSnapshot) -> String {
    let mut s = String::with_capacity(64 + snap.entities.len() * 96);
    s.push_str("{\n");
    s.push_str(&format!("  \"version\": {},\n", snap.version));
    s.push_str(&format!("  \"tick\": {},\n", snap.tick));
    s.push_str("  \"entities\": [\n");
    for (i, e) in snap.entities.iter().enumerate() {
        s.push_str(&format!(
            "    {{ \"id\": {}, \"pos\": [{:.3}, {:.3}, {:.3}], \"vel\": [{:.3}, {:.3}, {:.3}], \"yaw\": {:.4}, \"pitch\": {:.4}, \"seq\": {} }}",
            e.id, e.pos.x, e.pos.y, e.pos.z, e.vel.x, e.vel.y, e.vel.z, e.yaw, e.pitch, e.last_input_seq
        ));
        if i + 1 < snap.entities.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ]\n}\n");
    s
}

/// К сводке: сколько байт и сущностей в наборе снимков.
pub fn snapshot_stats(snap: &WorldSnapshot) -> BTreeMap<&'static str, u64> {
    let mut m = BTreeMap::new();
    m.insert("tick", snap.tick);
    m.insert("entities", snap.entities.len() as u64);
    m.insert("bytes", snap.to_bytes().len() as u64);
    m
}

// ===========================================================================
// Экономика: снимок инвентаря и рынка
// ===========================================================================

use ou_economy::{Inventory, ItemId, Market, Order, OrderId, OwnerId, Side, SystemId};

/// Магия файла экономики: "OUEC".
pub const ECON_MAGIC: [u8; 4] = *b"OUEC";
/// Версия формата экономики.
pub const ECON_VERSION: u16 = 1;

fn side_to_u8(s: Side) -> u8 {
    match s {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

fn side_from_u8(b: u8) -> Side {
    if b == 0 {
        Side::Buy
    } else {
        Side::Sell
    }
}

/// Снимок экономики: инвентарь всех владельцев и активные ордера рынка.
#[derive(Clone, Debug, PartialEq)]
pub struct EconomySnapshot {
    pub version: u16,
    /// (владелец, система, предмет, количество)
    pub stock: Vec<(OwnerId, SystemId, ItemId, u64)>,
    /// Активные ордера.
    pub orders: Vec<Order>,
    /// Следующий свободный id ордера.
    pub next_order_id: OrderId,
}

impl EconomySnapshot {
    /// Снять снимок с инвентаря и рынка.
    pub fn capture(inv: &Inventory, market: &Market) -> Self {
        let mut stock = Vec::new();
        for ((owner, system), bag) in inv.iter_slots() {
            for (item, qty) in bag {
                if *qty > 0 {
                    stock.push((*owner, *system, *item, *qty));
                }
            }
        }
        stock.sort_unstable();
        EconomySnapshot {
            version: ECON_VERSION,
            stock,
            orders: market.all_orders(),
            next_order_id: market.next_order_id(),
        }
    }

    /// Восстановить инвентарь и рынок.
    pub fn restore(&self) -> (Inventory, Market) {
        let mut inv = Inventory::new();
        for (owner, system, item, qty) in &self.stock {
            inv.add(*owner, *system, *item, *qty);
        }
        let mut market = Market::new();
        for o in &self.orders {
            market.restore_order(o.clone());
        }
        // Сохраняем счётчик id даже если ордеров нет.
        market.set_next_order_id(self.next_order_id);
        (inv, market)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(32 + self.stock.len() * 12 + self.orders.len() * 40);
        body.extend_from_slice(&ECON_MAGIC);
        body.extend_from_slice(&self.version.to_le_bytes());
        body.extend_from_slice(&(self.stock.len() as u32).to_le_bytes());
        body.extend_from_slice(&(self.orders.len() as u32).to_le_bytes());
        body.extend_from_slice(&self.next_order_id.to_le_bytes());
        for (owner, system, item, qty) in &self.stock {
            body.extend_from_slice(&owner.to_le_bytes());
            body.extend_from_slice(&system.to_le_bytes());
            body.extend_from_slice(&item.to_le_bytes());
            body.extend_from_slice(&qty.to_le_bytes());
        }
        for o in &self.orders {
            body.extend_from_slice(&o.id.to_le_bytes());
            body.extend_from_slice(&o.system.to_le_bytes());
            body.extend_from_slice(&o.item.to_le_bytes());
            body.extend_from_slice(&o.owner.to_le_bytes());
            body.push(side_to_u8(o.side));
            body.extend_from_slice(&o.price.to_le_bytes());
            body.extend_from_slice(&o.qty.to_le_bytes());
            body.extend_from_slice(&o.placed_tick.to_le_bytes());
        }
        let sum = fnv1a(&body);
        let mut out = body;
        out.extend_from_slice(&sum.to_le_bytes());
        out
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self, PersistError> {
        const TRAILER: usize = 8;
        if buf.len() < 4 + 2 + 4 + 4 + 8 + TRAILER {
            return Err(PersistError::Truncated { expected: 30, got: buf.len() });
        }
        if buf[0..4] != ECON_MAGIC {
            return Err(PersistError::BadMagic);
        }
        let (body, sum_bytes) = buf.split_at(buf.len() - TRAILER);
        let expected = u64::from_le_bytes(sum_bytes.try_into().unwrap());
        let got = fnv1a(body);
        if expected != got {
            return Err(PersistError::ChecksumMismatch { expected, got });
        }
        let version = u16::from_le_bytes([body[4], body[5]]);
        if version != ECON_VERSION {
            return Err(PersistError::UnsupportedVersion(version));
        }
        let n_stock = u32::from_le_bytes(body[6..10].try_into().unwrap()) as usize;
        let n_orders = u32::from_le_bytes(body[10..14].try_into().unwrap()) as usize;
        let next_order_id = u64::from_le_bytes(body[14..22].try_into().unwrap());
        let need = 22 + n_stock * 20 + n_orders * 45;
        if body.len() < need {
            return Err(PersistError::Truncated { expected: need, got: body.len() });
        }

        let mut off = 22;
        let mut stock = Vec::with_capacity(n_stock);
        for _ in 0..n_stock {
            let owner = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
            let system = u32::from_le_bytes(body[off + 4..off + 8].try_into().unwrap());
            let item = u32::from_le_bytes(body[off + 8..off + 12].try_into().unwrap());
            let qty = u64::from_le_bytes(body[off + 12..off + 20].try_into().unwrap());
            off += 20;
            stock.push((owner, system, item, qty));
        }
        let mut orders = Vec::with_capacity(n_orders);
        for _ in 0..n_orders {
            let id = u64::from_le_bytes(body[off..off + 8].try_into().unwrap());
            let system = u32::from_le_bytes(body[off + 8..off + 12].try_into().unwrap());
            let item = u32::from_le_bytes(body[off + 12..off + 16].try_into().unwrap());
            let owner = u32::from_le_bytes(body[off + 16..off + 20].try_into().unwrap());
            let side = side_from_u8(body[off + 20]);
            let price = u64::from_le_bytes(body[off + 21..off + 29].try_into().unwrap());
            let qty = u64::from_le_bytes(body[off + 29..off + 37].try_into().unwrap());
            let placed_tick = u64::from_le_bytes(body[off + 37..off + 45].try_into().unwrap());
            off += 45;
            orders.push(Order { id, system, item, owner, side, price, qty, placed_tick });
        }
        Ok(EconomySnapshot { version, stock, orders, next_order_id })
    }
}

/// Файловое хранилище экономики (отдельный слот рядом с миром).
#[derive(Clone, Debug)]
pub struct EconomyFileStore {
    dir: PathBuf,
}

impl EconomyFileStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PersistError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        Ok(EconomyFileStore { dir })
    }

    fn path(&self) -> PathBuf {
        self.dir.join("economy.snapshot")
    }

    fn tmp(&self) -> PathBuf {
        self.dir.join("economy.snapshot.tmp")
    }

    pub fn save(&self, snap: &EconomySnapshot) -> Result<u64, PersistError> {
        let bytes = snap.to_bytes();
        let n = bytes.len() as u64;
        {
            let mut f = std::fs::File::create(self.tmp())?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(self.tmp(), self.path())?;
        Ok(n)
    }

    pub fn load(&self) -> Result<Option<EconomySnapshot>, PersistError> {
        let p = self.path();
        if !p.exists() {
            return Ok(None);
        }
        let mut buf = Vec::new();
        std::fs::File::open(&p)?.read_to_end(&mut buf)?;
        Ok(Some(EconomySnapshot::from_bytes(&buf)?))
    }
}

/// Удобный экспорт экономики в JSON (для отладки).
pub fn economy_to_json(snap: &EconomySnapshot) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"stock\": {},\n", snap.stock.len()));
    s.push_str(&format!("  \"orders\": {},\n", snap.orders.len()));
    s.push_str(&format!("  \"next_order_id\": {}\n", snap.next_order_id));
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use ou_sim::World;

    fn world_with(n: u32) -> World {
        let mut w = World::new();
        for i in 0..n {
            w.spawn(i + 1, Vec3::new(i as f64 * 10.0, 1.5, -3.0));
        }
        w.tick = 42;
        w
    }

    #[test]
    fn snapshot_roundtrip_is_exact() {
        let w = world_with(5);
        let snap = WorldSnapshot::capture(&w);
        let bytes = snap.to_bytes();
        let back = WorldSnapshot::from_bytes(&bytes).unwrap();
        assert_eq!(snap, back);
        // Восстановленный мир должен иметь ту же контрольную сумму.
        assert_eq!(w.checksum(), back.restore().checksum());
        assert_eq!(back.restore().tick, 42);
    }

    #[test]
    fn corrupted_bytes_are_rejected() {
        let w = world_with(3);
        let mut bytes = WorldSnapshot::capture(&w).to_bytes();
        let n = bytes.len();
        bytes[n / 2] ^= 0xFF; // портим середину
        let err = WorldSnapshot::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, PersistError::ChecksumMismatch { .. }));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let bytes = vec![0u8; 64];
        assert!(matches!(
            WorldSnapshot::from_bytes(&bytes).unwrap_err(),
            PersistError::BadMagic
        ));
    }

    #[test]
    fn truncated_file_is_rejected() {
        let w = world_with(10);
        let mut bytes = WorldSnapshot::capture(&w).to_bytes();
        bytes.truncate(bytes.len() - 20);
        let err = WorldSnapshot::from_bytes(&bytes).unwrap_err();
        // Либо обрезание, либо несовпадение суммы — оба корректны.
        assert!(matches!(
            err,
            PersistError::Truncated { .. } | PersistError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn file_store_saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("ou-persist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = FileStore::open(&dir).unwrap();

        // Пустое хранилище.
        assert!(store.load().unwrap().is_none());

        // Сохраняем и загружаем.
        let w = world_with(4);
        save_world(&mut store, &w).unwrap();
        let loaded = load_world(&store).unwrap();
        assert_eq!(loaded.checksum(), w.checksum());
        assert_eq!(loaded.entities.len(), 4);

        // Второе сохранение создаёт резервную копию.
        let mut w2 = w.clone();
        w2.spawn(99, Vec3::new(1.0, 2.0, 3.0));
        save_world(&mut store, &w2).unwrap();
        assert!(dir.join("world.snapshot.prev").exists());
        let loaded2 = load_world(&store).unwrap();
        assert_eq!(loaded2.entities.len(), 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_store_recovers_from_backup() {
        let dir = std::env::temp_dir().join(format!("ou-persist-bkp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = FileStore::open(&dir).unwrap();

        let w1 = world_with(2);
        save_world(&mut store, &w1).unwrap();
        let w2 = world_with(6);
        save_world(&mut store, &w2).unwrap(); // создаёт backup (со 2 сущностями)

        // Портим основной файл.
        std::fs::write(dir.join("world.snapshot"), b"garbage-not-a-snapshot").unwrap();

        // Загрузка должна вернуться к резервной копии (2 сущности).
        let recovered = load_world(&store).unwrap();
        assert_eq!(recovered.entities.len(), 2, "должны восстановиться из backup");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_store_counts_saves() {
        let mut store = MemoryStore::default();
        let w = world_with(1);
        save_world(&mut store, &w).unwrap();
        save_world(&mut store, &w).unwrap();
        assert_eq!(store.saves, 2);
        assert!(store.load().unwrap().is_some());
    }

    #[test]
    fn empty_world_snapshot_is_valid() {
        let w = World::new();
        let snap = WorldSnapshot::capture(&w);
        assert!(snap.entities.is_empty());
        let bytes = snap.to_bytes();
        let back = WorldSnapshot::from_bytes(&bytes).unwrap();
        assert_eq!(back.entities.len(), 0);
        assert_eq!(back.tick, 0);
    }

    #[test]
    fn json_export_contains_entities() {
        let w = world_with(2);
        let json = to_json(&WorldSnapshot::capture(&w));
        assert!(json.contains("\"entities\""));
        assert!(json.contains("\"id\": 1"));
        assert!(json.contains("\"id\": 2"));
    }

    // --- экономика ---

    fn econ_fixture() -> (Inventory, Market) {
        use ou_economy::{MatchingEngine, Side};
        let mut inv = Inventory::new();
        inv.add(1, 10, 5, 100);
        inv.add(2, 10, 5, 50);
        inv.add(1, 10, 7, 20);
        let mut eng = MatchingEngine::new(Market::new(), inv);
        // Оставляем неисполненные ордера в стакане.
        eng.place_sell(10, 5, 1, 30, 10, 1).unwrap();
        eng.place_buy(10, 7, 2, 15, 5, 2).unwrap();
        let _ = Side::Buy;
        // Возвращаем инвентарь и рынок как есть.
        (eng.inventory, eng.market)
    }

    #[test]
    fn economy_snapshot_roundtrip() {
        let (inv, market) = econ_fixture();
        let snap = EconomySnapshot::capture(&inv, &market);
        let bytes = snap.to_bytes();
        let back = EconomySnapshot::from_bytes(&bytes).unwrap();
        assert_eq!(snap, back);

        // Восстановленный инвентарь и рынок сохраняют объёмы.
        let (inv2, market2) = back.restore();
        assert_eq!(inv2.amount(1, 10, 5), inv.amount(1, 10, 5));
        assert_eq!(inv2.amount(2, 10, 5), inv.amount(2, 10, 5));
        assert_eq!(
            market2.best_ask(10, 5).map(|l| l.qty),
            market.best_ask(10, 5).map(|l| l.qty)
        );
        assert_eq!(
            market2.best_bid(10, 7).map(|l| l.price),
            market.best_bid(10, 7).map(|l| l.price)
        );
    }

    #[test]
    fn economy_snapshot_preserves_order_counter() {
        let (inv, market) = econ_fixture();
        let next_before = market.next_order_id();
        let snap = EconomySnapshot::capture(&inv, &market);
        let (_, market2) = EconomySnapshot::from_bytes(&snap.to_bytes()).unwrap().restore();
        assert_eq!(market2.next_order_id(), next_before, "счётчик id ордеров сохранён");
    }

    #[test]
    fn economy_snapshot_rejects_corruption() {
        let (inv, market) = econ_fixture();
        let mut bytes = EconomySnapshot::capture(&inv, &market).to_bytes();
        let n = bytes.len();
        bytes[n / 2] ^= 0xAA;
        assert!(matches!(
            EconomySnapshot::from_bytes(&bytes).unwrap_err(),
            PersistError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn economy_file_store_saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("ou-econ-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = EconomyFileStore::open(&dir).unwrap();
        assert!(store.load().unwrap().is_none());

        let (inv, market) = econ_fixture();
        let snap = EconomySnapshot::capture(&inv, &market);
        let n = store.save(&snap).unwrap();
        assert!(n > 0);
        let back = store.load().unwrap().unwrap();
        assert_eq!(back, snap);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_economy_snapshot_is_valid() {
        let snap = EconomySnapshot::capture(&Inventory::new(), &Market::new());
        assert!(snap.stock.is_empty());
        assert!(snap.orders.is_empty());
        let back = EconomySnapshot::from_bytes(&snap.to_bytes()).unwrap();
        assert_eq!(back, snap);
        assert_eq!(back.next_order_id, 1);
    }
}
