//! # ou-protocol
//!
//! Бинарный сетевой протокол Open Universe (клиент ↔ сервер).
//!
//! Принципы:
//! * **Детерминизм.** Все вычисления — целочисленные или базовые IEEE‑754
//!   операции (`+ - * /`), без платформо‑зависимых `libm`‑функций.
//! * **Квантование.** Позиции передаются как `i32` с масштабом
//!   [`POS_SCALE`] (1 мм), что даёт диапазон ±2147 км вокруг начала отсчёта.
//!   В паре с floating origin этого достаточно для локальной зоны и даже для
//!   планеты радиусом 500 км.
//! * **Дельта‑кодирование.** Снимок содержит только изменившиеся сущности.
//! * **Единый контракт.** Одна схема — общая для C++ (UE5) и Rust.

use std::collections::BTreeMap;

/// Масштаб позиции: 1 единица провода = 1e-3 игровых единиц (1 мм).
///
/// Диапазон `i32` при таком масштабе — ±2 147 км. Это важно: домашняя планета
/// имеет радиус 500 км, и при прежнем масштабе (0.1 мм, ±214 км) позиции на
/// поверхности **переполнялись**. Точность 1 мм для игрового мира достаточна.
pub const POS_SCALE: f64 = 1_000.0;
/// Масштаб нормализованных величин (−1..1): i16.
pub const UNIT_SCALE: f32 = 32_767.0;

/// Числовой идентификатор сущности (сетевой).
pub type NetId = u32;

/// Количество тиков в секунду симуляции.
pub const TICK_HZ: u32 = 30;
/// Длительность одного тика в секундах.
pub const TICK_DT: f64 = 1.0 / TICK_HZ as f64;

// ---------------------------------------------------------------------------
// Детерминированная математика
// ---------------------------------------------------------------------------

/// Вектор в игровом пространстве. Используется `f64` и только базовые операции.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    #[inline]
    pub fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }

    #[inline]
    pub fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }

    #[inline]
    pub fn scale(self, k: f64) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }

    #[inline]
    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    #[inline]
    pub fn distance(self, o: Vec3) -> f64 {
        self.sub(o).length()
    }

    /// Скалярное произведение.
    #[inline]
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Векторное произведение.
    #[inline]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Квантование в провод (i32, масштаб [`POS_SCALE`]).
    pub fn to_wire(self) -> [i32; 3] {
        [
            (self.x * POS_SCALE).round() as i32,
            (self.y * POS_SCALE).round() as i32,
            (self.z * POS_SCALE).round() as i32,
        ]
    }

    /// Восстановление из провода.
    pub fn from_wire(w: [i32; 3]) -> Vec3 {
        Vec3::new(
            w[0] as f64 / POS_SCALE,
            w[1] as f64 / POS_SCALE,
            w[2] as f64 / POS_SCALE,
        )
    }
}

// ---------------------------------------------------------------------------
// Ввод игрока
// ---------------------------------------------------------------------------

/// Кадр управления от клиента.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct InputFrame {
    /// Монотонный номер кадра (используется для реконсиляции).
    pub seq: u32,
    /// Тяга вперёд, −1..1.
    pub throttle: f32,
    /// Поворот вокруг вертикальной оси, −1..1.
    pub yaw: f32,
    /// Тангаж, −1..1.
    pub pitch: f32,
    /// Стрейф (вбок: вправо положительно), −1..1.
    pub strafe: f32,
    /// Подъём/спуск вдоль локального «верха», −1..1.
    pub up: f32,
    /// Огонь.
    pub fire: bool,
    /// Сверхсвет (сверхскоростной режим): межпланетный перелёт.
    pub boost: bool,
}

impl InputFrame {
    pub fn neutral(seq: u32) -> Self {
        InputFrame { seq, ..Default::default() }
    }
}

// ---------------------------------------------------------------------------
// Состояние сущности
// ---------------------------------------------------------------------------

/// Авторитетное состояние одной сущности, передаваемое по сети.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub id: NetId,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Углы Эйлера (рад) — для MVP реплицируем как есть.
    pub yaw: f32,
    pub pitch: f32,
}

// ---------------------------------------------------------------------------
// Сообщения
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// Первый пакет: клиент представляется.
    ClientHello { protocol: u16, name: String },
    /// Кадр управления (несколько подряд для надёжности).
    ClientInput { inputs: Vec<InputFrame> },
    /// Команда строительства: поставить, удалить или покрасить воксель.
    ClientBuild(BuildCommand),
    /// Снимок состояния с сервера.
    ServerSnapshot(Snapshot),
    /// Подтверждение: какой последний ввод сервер применил.
    ServerAck { last_input_seq: u32, server_tick: u64 },
    /// Ответ на команду строительства.
    ServerBuildResult { accepted: bool, reason: BuildReason },
    /// Приветствие сервера: какой сущностью управляет этот клиент.
    ///
    /// Отправляется сразу после `ClientHello`, чтобы клиент знал свой id
    /// (иначе он не может отличить свой корабль от чужих в снимке).
    ServerWelcome { entity: NetId },
    /// Применённая правка конструкции (репликация другим игрокам).
    ///
    /// Сервер рассылает её всем, кто видит конструкцию, — так у всех
    /// одинаковая постройка. `revision` — номер правки, по нему клиент
    /// подтверждает приём и не применяет одно и то же дважды.
    ServerBuildEdit(BuildEdit),
    /// Подтверждение правки клиентом (чтобы сервер не слал её снова).
    ClientBuildAck { entity: NetId, revision: u64 },
    /// Владелец конструкции разрешает строить другому игроку.
    ClientGrantBuild { entity: NetId, player: NetId },
    /// Сообщение клиенту, может ли он редактировать конструкцию.
    ServerBuildAccess { entity: NetId, can_edit: bool },
    /// Сервер: состоялся выстрел (для трассера и эффектов у всех рядом).
    ServerShot(ShotEvent),
    /// Сервер: текущие щит и броня сущности (для HUD и состояния цели).
    ServerHull { entity: NetId, shield: u32, armor: u32, destroyed: bool },
    /// Сервер: сущность уничтожена, возрождение ожидается на тике `respawn_tick`.
    ServerDeath { entity: NetId, respawn_tick: u64 },
    /// Сервер: сущность возрождена (полный корпус), неуязвима до `invuln_until`.
    ServerRespawn { entity: NetId, invuln_until: u64 },
    /// Клиент: поза пешего персонажа (ходьба по поверхности).
    ///
    /// Клиент сам ведёт ходьбу (локальная симуляция по той же формуле, что
    /// сервер), а сервер лишь принимает позу и раздаёт её другим. `active =
    /// false` означает, что игрок снова в корабле.
    ClientCharacter { entity: NetId, pos: Vec3, yaw: f32, moving: bool, active: bool },
    /// Сервер: поза чужого пешего персонажа (для отрисовки фигурки).
    ServerCharacter { entity: NetId, pos: Vec3, yaw: f32, moving: bool },
    /// Сервер: описание звёздной системы (солнце и планеты) для отрисовки.
    ///
    /// Отправляется при подключении. Позиции — в СЕРВЕРНЫХ единицах,
    /// относительно центра домашней планеты. Так клиент не хардкодит систему,
    /// а рисует то, что сгенерировал сервер.
    ServerSystem { id: u32, name: String, bodies: Vec<SystemBodyDto> },
    /// Клиент: копать грунт в направлении взгляда.
    ///
    /// `slot` — номер воронки на планете (слот в списке деформаций),
    /// `dir` — направление из центра планеты (единичное), `cos_radius` —
    /// косинус углового радиуса воронки, `depth` — глубина в единицах длины.
    ClientDig { slot: u32, dir: Vec3, cos_radius: f64, depth: f64 },
    /// Сервер: применённая воронка (репликация всем, включая автора).
    ///
    /// Клиент вставляет её в свой список деформаций и перестраивает рельеф.
    ServerTerrainDig { slot: u32, dir: Vec3, cos_radius: f64, depth: f64 },
    /// Клиент: разместить ордер на рынке.
    ///
    /// `side` 0 — покупка, 1 — продажа. `item` — id предмета, `price` — цена за
    /// единицу, `qty` — количество. Сервер авторитетно исполняет против стакана.
    ClientMarket { side: u8, item: u32, price: u64, qty: u64 },
    /// Сервер: состояние экономики игрока (для HUD и окна рынка).
    ///
    /// `credits` — кредиты, `items` — инвентарь (id, количество), `bids`/`asks`
    /// — верх стакана по каждому предмету (цена, объём). Шлётся при изменении.
    ServerMarket {
        credits: u64,
        items: Vec<(u32, u64)>,
        bids: Vec<(u32, u64, u64)>,
        asks: Vec<(u32, u64, u64)>,
    },
    /// Сервер: результат копания игроку (что добыто и на какой глубине).
    ///
    /// Шлётся автору удара: клиент показывает, что именно он выкопал (`item`),
    /// сколько (`qty`) и с какого слоя глубины (`layer`: 0 руда, 1 лёд, 2
    /// кристалл, 3 металл), плюс текущую глубину ямы (`hole_depth`).
    ServerDigYield { item: u32, qty: u64, layer: u8, hole_depth: f64 },
    /// Клиент: запрос пинга (эхо). `nonce` клиент выбирает сам.
    ///
    /// Сервер немедленно отвечает `ServerPong` с тем же `nonce` — клиент считает
    /// реальную задержку по кругу (RTT). Нужен и пешком, и в корабле.
    ClientPing { nonce: u32 },
    /// Сервер: ответ на `ClientPing` — эхо `nonce`.
    ServerPong { nonce: u32 },
    /// Сервер: список рецептов производства (для окна «Производство»).
    ///
    /// Каждый рецепт: id, имя, входы (id, количество), выход (id, количество),
    /// время в тиках.
    ServerRecipes { recipes: Vec<(u32, String, Vec<(u32, u64)>, (u32, u64), u64)> },
    /// Клиент: выполнить рецепт производства.
    ClientCraft { recipe: u32 },
    /// Сервер: результат крафта — что получилось, или причина отказа.
    ServerCraftResult { ok: bool, item: u32, qty: u64, reason: String },
    /// Сервер: список территорий системы (владение, статус, налог) для HUD.
    ///
    /// Шлётся при подключении и при каждом изменении (захват, смена статуса).
    /// Позиции — в СЕРВЕРНЫХ единицах, как и в `ServerSystem`.
    ServerTerritory { territories: Vec<TerritoryDto> },
    /// Клиент: объявить территорию (по id) своей — захват.
    ///
    /// Сервер проверяет, что игрок **внутри** территории, и передаёт владение
    /// ему (статус — «спорная»). Возвращает новое состояние всем.
    ClientCapture { id: u32 },
}

/// Территория в сообщении (для HUD).
#[derive(Clone, Debug, PartialEq)]
pub struct TerritoryDto {
    pub id: u32,
    /// Владелец: 0 — ничей, иначе id игрока-владельца.
    pub owner: u32,
    /// Статус: 0 мирная, 1 спорная, 2 укреплённая.
    pub status: u8,
    pub center: Vec3,
    pub radius: f64,
    /// Щит контроля (0..100).
    pub control_shield: u32,
    /// Налог с торговли, проценты.
    pub tax_percent: u32,
    /// Сколько налога уже собрано владельцем.
    pub collected_tax: u64,
}

/// Тело системы в сообщении.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemBodyDto {
    pub id: u32,
    /// Тип: 0 звезда, 1 планета, 2 луна, 3 газовый гигант, 4 пояс, 5 чёрная дыра.
    pub kind: u8,
    /// Тип поверхности: 0 землеподобная, 1 пустынная, 2 ледяная, 3 вулканическая,
    /// 4 газовый гигант, 5 безвоздушная. Задаёт палитру планеты на клиенте.
    pub surface: u8,
    /// Радиус тела, в серверных единицах (подсказка; клиент может задать свой).
    pub radius: f64,
    /// Центр орбиты (для неподвижных тел — их позиция), серверные единицы.
    pub center: Vec3,
    /// Радиус орбиты, серверные единицы (0 — тело неподвижно).
    pub orbit: f32,
    /// Начальная фаза орбиты, радианы.
    pub phase: f32,
    /// Угловая скорость орбиты, рад/с.
    pub omega: f32,
}

/// Состоявшийся выстрел (сервер → клиенты).
///
/// Авторитетный: сервер сам решает, попал ли выстрел и какой урон прошёл,
/// и рассылает событие. Клиент лишь рисует трассер/вспышку — «доверия» к
/// клиентскому попаданию нет (античит).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotEvent {
    /// Кто стрелял.
    pub attacker: NetId,
    /// По кому (0 — промах без цели / выстрел в пустоту).
    pub target: NetId,
    /// Попал ли.
    pub hit: bool,
    /// Прошло урона всего (щит + броня).
    pub damage: u32,
    /// Цель уничтожена этим выстрелом.
    pub destroyed: bool,
}

/// Применённая правка конструкции (сервер → клиенты).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildEdit {
    /// Конструкция, к которой относится правка.
    pub entity: NetId,
    /// Кто её применил.
    pub author: NetId,
    pub action: BuildAction,
    pub coord: (i32, i32, i32),
    pub color: Color,
    /// Монотонный номер правки внутри конструкции (начиная с 1).
    pub revision: u64,
}

/// Действие редактора.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildAction {
    Place,
    Remove,
    Paint,
}

/// Команда строительства (то, что шлёт клиент).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildCommand {
    pub entity: NetId,
    pub action: BuildAction,
    /// Координаты вокселя в сетке конструкции.
    pub coord: (i32, i32, i32),
    /// Цвет (для Place/Paint).
    pub color: Color,
}

/// Цвет вокселя на проводе.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

/// Почему команда отклонена.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildReason {
    Ok,
    /// Ячейка занята / пуста / вне границ и т. п. — обобщённо.
    Invalid,
    /// Нарушена целостность конструкции.
    Disconnected,
    /// Достигнут лимит вокселей.
    LimitReached,
}

/// Снимок мира: базовая линия + дельта.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Актуальный тик симуляции.
    pub tick: u64,
    /// Подтверждённый номер кадра ввода клиента.
    pub ack_input_seq: u32,
    /// Полная базовая линия (при `baseline == true`) либо только дельта.
    pub baseline: bool,
    /// Изменившиеся (или все) сущности.
    pub entities: Vec<EntityState>,
}

// ---------------------------------------------------------------------------
// Бинарная сериализация (little-endian)
// ---------------------------------------------------------------------------

mod wire {
    use super::*;

    pub struct Writer {
        pub buf: Vec<u8>,
    }

    impl Writer {
        pub fn new() -> Self {
            Writer { buf: Vec::with_capacity(256) }
        }
        pub fn u8(&mut self, v: u8) {
            self.buf.push(v);
        }
        pub fn bool(&mut self, v: bool) {
            self.u8(if v { 1 } else { 0 });
        }
        pub fn u16(&mut self, v: u16) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn u32(&mut self, v: u32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn u64(&mut self, v: u64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn i32(&mut self, v: i32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn i16(&mut self, v: i16) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn f32(&mut self, v: f32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn f64(&mut self, v: f64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn str(&mut self, s: &str) {
            let b = s.as_bytes();
            self.u16(b.len() as u16);
            self.buf.extend_from_slice(b);
        }
        /// Varint (LEB128): маленькие числа занимают 1 байт вместо 8.
        pub fn varint(&mut self, mut v: u64) {
            while v >= 0x80 {
                self.buf.push((v as u8) | 0x80);
                v >>= 7;
            }
            self.buf.push(v as u8);
        }
        /// Знаковый varint (zigzag): −1 → 1, 1 → 2, поэтому мелкие модули дешевы.
        pub fn svarint(&mut self, v: i64) {
            let zz = ((v << 1) ^ (v >> 63)) as u64;
            self.varint(zz);
        }
        pub fn vec3_q(&mut self, v: Vec3) {
            for c in v.to_wire() {
                self.i32(c);
            }
        }
        /// Вектор через zigzag-varint: компактнее для умеренных координат.
        pub fn vec3_qv(&mut self, v: Vec3) {
            for c in v.to_wire() {
                self.svarint(c as i64);
            }
        }
        pub fn unit(&mut self, v: f32) {
            let q = (v.clamp(-1.0, 1.0) * UNIT_SCALE).round() as i16;
            self.i16(q);
        }
    }

    pub struct Reader<'a> {
        pub buf: &'a [u8],
        pub pos: usize,
    }

    #[derive(Debug)]
    pub enum WireError {
        UnexpectedEof,
        BadTag(u8),
        Utf8,
        TooLarge(usize),
        VarintOverflow,
    }

    impl<'a> Reader<'a> {
        pub fn new(buf: &'a [u8]) -> Self {
            Reader { buf, pos: 0 }
        }
        fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
            if self.pos + n > self.buf.len() {
                return Err(WireError::UnexpectedEof);
            }
            let s = &self.buf[self.pos..self.pos + n];
            self.pos += n;
            Ok(s)
        }
        pub fn u8(&mut self) -> Result<u8, WireError> {
            Ok(self.take(1)?[0])
        }
        pub fn bool(&mut self) -> Result<bool, WireError> {
            Ok(self.u8()? != 0)
        }
        pub fn u16(&mut self) -> Result<u16, WireError> {
            let b = self.take(2)?;
            Ok(u16::from_le_bytes([b[0], b[1]]))
        }
        pub fn u32(&mut self) -> Result<u32, WireError> {
            let b = self.take(4)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        pub fn u64(&mut self) -> Result<u64, WireError> {
            let b = self.take(8)?;
            Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        }
        pub fn i32(&mut self) -> Result<i32, WireError> {
            let b = self.take(4)?;
            Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        pub fn i16(&mut self) -> Result<i16, WireError> {
            let b = self.take(2)?;
            Ok(i16::from_le_bytes([b[0], b[1]]))
        }
        pub fn f32(&mut self) -> Result<f32, WireError> {
            let b = self.take(4)?;
            Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        pub fn f64(&mut self) -> Result<f64, WireError> {
            let b = self.take(8)?;
            Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        }
        pub fn str(&mut self) -> Result<String, WireError> {
            let n = self.u16()? as usize;
            let b = self.take(n)?;
            String::from_utf8(b.to_vec()).map_err(|_| WireError::Utf8)
        }
        pub fn varint(&mut self) -> Result<u64, WireError> {
            let mut result: u64 = 0;
            let mut shift = 0u32;
            loop {
                let b = self.u8()?;
                result |= ((b & 0x7F) as u64) << shift;
                if b & 0x80 == 0 {
                    return Ok(result);
                }
                shift += 7;
                if shift >= 64 {
                    return Err(WireError::VarintOverflow);
                }
            }
        }
        pub fn svarint(&mut self) -> Result<i64, WireError> {
            let zz = self.varint()?;
            Ok(((zz >> 1) as i64) ^ -((zz & 1) as i64))
        }
        pub fn vec3_q(&mut self) -> Result<Vec3, WireError> {
            Ok(Vec3::from_wire([self.i32()?, self.i32()?, self.i32()?]))
        }
        pub fn vec3_qv(&mut self) -> Result<Vec3, WireError> {
            let x = self.svarint()? as i32;
            let y = self.svarint()? as i32;
            let z = self.svarint()? as i32;
            Ok(Vec3::from_wire([x, y, z]))
        }
        pub fn unit(&mut self) -> Result<f32, WireError> {
            Ok(self.i16()? as f32 / UNIT_SCALE)
        }
    }

    const TAG_HELLO: u8 = 1;
    const TAG_INPUT: u8 = 2;
    const TAG_SNAPSHOT: u8 = 3;
    const TAG_ACK: u8 = 4;
    const TAG_SNAPSHOT_V2: u8 = 5;
    const TAG_BUILD: u8 = 6;
    const TAG_BUILD_RESULT: u8 = 7;
    const TAG_WELCOME: u8 = 8;
    const TAG_BUILD_EDIT: u8 = 9;
    const TAG_BUILD_ACK: u8 = 10;
    const TAG_BUILD_GRANT: u8 = 11;
    const TAG_BUILD_ACCESS: u8 = 12;
    const TAG_SHOT: u8 = 13;
    const TAG_HULL: u8 = 14;
    const TAG_DEATH: u8 = 15;
    const TAG_RESPAWN: u8 = 16;
    const TAG_CLIENT_CHARACTER: u8 = 17;
    const TAG_SERVER_CHARACTER: u8 = 18;
    const TAG_SYSTEM: u8 = 19;
    const TAG_DIG: u8 = 20;
    const TAG_MARKET_ORDER: u8 = 21;
    const TAG_MARKET_STATE: u8 = 22;
    const TAG_DIG_YIELD: u8 = 23;
    const TAG_PING: u8 = 24;
    const TAG_PONG: u8 = 25;
    const TAG_RECIPES: u8 = 26;
    const TAG_CRAFT: u8 = 27;
    const TAG_CRAFT_RESULT: u8 = 28;
    const TAG_TERRITORY: u8 = 29;
    const TAG_CAPTURE: u8 = 30;

    pub fn encode(msg: &Message) -> Vec<u8> {
        let mut w = Writer::new();
        match msg {
            Message::ClientHello { protocol, name } => {
                w.u8(TAG_HELLO);
                w.u16(*protocol);
                w.str(name);
            }
            Message::ClientInput { inputs } => {
                w.u8(TAG_INPUT);
                w.u16(inputs.len() as u16);
                for i in inputs {
                    w.u32(i.seq);
                    w.unit(i.throttle);
                    w.unit(i.yaw);
                    w.unit(i.pitch);
                    w.unit(i.strafe);
                    w.unit(i.up);
                    w.bool(i.fire);
                    w.bool(i.boost);
                }
            }
            Message::ServerSnapshot(s) => {
                w.u8(TAG_SNAPSHOT_V2);
                w.varint(s.tick);
                w.varint(s.ack_input_seq as u64);
                w.bool(s.baseline);
                w.varint(s.entities.len() as u64);
                for e in &s.entities {
                    w.varint(e.id as u64);
                    w.vec3_qv(e.pos);
                    w.vec3_qv(e.vel);
                    w.f32(e.yaw);
                    w.f32(e.pitch);
                }
            }
            Message::ServerAck { last_input_seq, server_tick } => {
                w.u8(TAG_ACK);
                w.u32(*last_input_seq);
                w.u64(*server_tick);
            }
            Message::ClientBuild(cmd) => {
                w.u8(TAG_BUILD);
                w.u32(cmd.entity);
                w.u8(match cmd.action {
                    BuildAction::Place => 0,
                    BuildAction::Remove => 1,
                    BuildAction::Paint => 2,
                });
                w.i32(cmd.coord.0);
                w.i32(cmd.coord.1);
                w.i32(cmd.coord.2);
                w.u8(cmd.color.0);
                w.u8(cmd.color.1);
                w.u8(cmd.color.2);
            }
            Message::ServerBuildResult { accepted, reason } => {
                w.u8(TAG_BUILD_RESULT);
                w.bool(*accepted);
                w.u8(match reason {
                    BuildReason::Ok => 0,
                    BuildReason::Invalid => 1,
                    BuildReason::Disconnected => 2,
                    BuildReason::LimitReached => 3,
                });
            }
            Message::ServerWelcome { entity } => {
                w.u8(TAG_WELCOME);
                w.varint(*entity as u64);
            }
            Message::ServerBuildEdit(e) => {
                w.u8(TAG_BUILD_EDIT);
                w.varint(e.entity as u64);
                w.varint(e.author as u64);
                w.u8(match e.action {
                    BuildAction::Place => 0,
                    BuildAction::Remove => 1,
                    BuildAction::Paint => 2,
                });
                w.i32(e.coord.0);
                w.i32(e.coord.1);
                w.i32(e.coord.2);
                w.u8(e.color.0);
                w.u8(e.color.1);
                w.u8(e.color.2);
                w.varint(e.revision);
            }
            Message::ClientBuildAck { entity, revision } => {
                w.u8(TAG_BUILD_ACK);
                w.varint(*entity as u64);
                w.varint(*revision);
            }
            Message::ClientGrantBuild { entity, player } => {
                w.u8(TAG_BUILD_GRANT);
                w.varint(*entity as u64);
                w.varint(*player as u64);
            }
            Message::ServerBuildAccess { entity, can_edit } => {
                w.u8(TAG_BUILD_ACCESS);
                w.varint(*entity as u64);
                w.bool(*can_edit);
            }
            Message::ServerShot(e) => {
                w.u8(TAG_SHOT);
                w.varint(e.attacker as u64);
                w.varint(e.target as u64);
                w.bool(e.hit);
                w.varint(e.damage as u64);
                w.bool(e.destroyed);
            }
            Message::ServerHull { entity, shield, armor, destroyed } => {
                w.u8(TAG_HULL);
                w.varint(*entity as u64);
                w.varint(*shield as u64);
                w.varint(*armor as u64);
                w.bool(*destroyed);
            }
            Message::ServerDeath { entity, respawn_tick } => {
                w.u8(TAG_DEATH);
                w.varint(*entity as u64);
                w.varint(*respawn_tick);
            }
            Message::ServerRespawn { entity, invuln_until } => {
                w.u8(TAG_RESPAWN);
                w.varint(*entity as u64);
                w.varint(*invuln_until);
            }
            Message::ClientCharacter { entity, pos, yaw, moving, active } => {
                w.u8(TAG_CLIENT_CHARACTER);
                w.varint(*entity as u64);
                w.vec3_qv(*pos);
                w.f32(*yaw);
                w.bool(*moving);
                w.bool(*active);
            }
            Message::ServerCharacter { entity, pos, yaw, moving } => {
                w.u8(TAG_SERVER_CHARACTER);
                w.varint(*entity as u64);
                w.vec3_qv(*pos);
                w.f32(*yaw);
                w.bool(*moving);
            }
            Message::ServerSystem { id, name, bodies } => {
                w.u8(TAG_SYSTEM);
                w.varint(*id as u64);
                w.str(name);
                w.varint(bodies.len() as u64);
                for b in bodies {
                    w.varint(b.id as u64);
                    w.u8(b.kind);
                    w.u8(b.surface);
                    // ВАЖНО: позиции тел системы — за пределами ±214 км, которые
                    // выражает штатное квантование (i32, 0.1 мм). Поэтому шлём их
                    // как f32 в серверных единицах: диапазона и точности хватает.
                    w.f32(b.center.x as f32);
                    w.f32(b.center.y as f32);
                    w.f32(b.center.z as f32);
                    w.f32(b.orbit);
                    w.f32(b.phase);
                    w.f32(b.omega);
                    w.f64(b.radius);
                }
            }
            Message::ClientDig { slot, dir, cos_radius, depth } => {
                w.u8(TAG_DIG);
                w.bool(false);   // 0 = запрос клиента
                w.u32(*slot);
                w.f64(dir.x);
                w.f64(dir.y);
                w.f64(dir.z);
                w.f64(*cos_radius);
                w.f64(*depth);
            }
            Message::ServerTerrainDig { slot, dir, cos_radius, depth } => {
                w.u8(TAG_DIG);
                w.bool(true);    // 1 = применённая воронка от сервера
                w.u32(*slot);
                w.f64(dir.x);
                w.f64(dir.y);
                w.f64(dir.z);
                w.f64(*cos_radius);
                w.f64(*depth);
            }
            Message::ClientMarket { side, item, price, qty } => {
                w.u8(TAG_MARKET_ORDER);
                w.u8(*side);
                w.varint(*item as u64);
                w.varint(*price);
                w.varint(*qty);
            }
            Message::ServerMarket { credits, items, bids, asks } => {
                w.u8(TAG_MARKET_STATE);
                w.varint(*credits);
                w.varint(items.len() as u64);
                for (id, qty) in items {
                    w.varint(*id as u64);
                    w.varint(*qty);
                }
                w.varint(bids.len() as u64);
                for (id, price, qty) in bids {
                    w.varint(*id as u64);
                    w.varint(*price);
                    w.varint(*qty);
                }
                w.varint(asks.len() as u64);
                for (id, price, qty) in asks {
                    w.varint(*id as u64);
                    w.varint(*price);
                    w.varint(*qty);
                }
            }
            Message::ServerDigYield { item, qty, layer, hole_depth } => {
                w.u8(TAG_DIG_YIELD);
                w.varint(*item as u64);
                w.varint(*qty);
                w.u8(*layer);
                w.f64(*hole_depth);
            }
            Message::ClientPing { nonce } => {
                w.u8(TAG_PING);
                w.u32(*nonce);
            }
            Message::ServerPong { nonce } => {
                w.u8(TAG_PONG);
                w.u32(*nonce);
            }
            Message::ServerRecipes { recipes } => {
                w.u8(TAG_RECIPES);
                w.varint(recipes.len() as u64);
                for (id, name, inputs, (oitem, oqty), ticks) in recipes {
                    w.varint(*id as u64);
                    w.str(name);
                    w.varint(inputs.len() as u64);
                    for (item, qty) in inputs {
                        w.varint(*item as u64);
                        w.varint(*qty);
                    }
                    w.varint(*oitem as u64);
                    w.varint(*oqty);
                    w.varint(*ticks);
                }
            }
            Message::ClientCraft { recipe } => {
                w.u8(TAG_CRAFT);
                w.varint(*recipe as u64);
            }
            Message::ServerCraftResult { ok, item, qty, reason } => {
                w.u8(TAG_CRAFT_RESULT);
                w.bool(*ok);
                w.varint(*item as u64);
                w.varint(*qty);
                w.str(reason);
            }
            Message::ServerTerritory { territories } => {
                w.u8(TAG_TERRITORY);
                w.varint(territories.len() as u64);
                for t in territories {
                    w.varint(t.id as u64);
                    w.varint(t.owner as u64);
                    w.u8(t.status);
                    w.vec3_qv(t.center);
                    w.f64(t.radius);
                    w.varint(t.control_shield as u64);
                    w.varint(t.tax_percent as u64);
                    w.varint(t.collected_tax);
                }
            }
            Message::ClientCapture { id } => {
                w.u8(TAG_CAPTURE);
                w.varint(*id as u64);
            }
        }
        w.buf
    }

    pub fn decode(buf: &[u8]) -> Result<Message, WireError> {
        let mut r = Reader::new(buf);
        let tag = r.u8()?;
        match tag {
            TAG_HELLO => Ok(Message::ClientHello { protocol: r.u16()?, name: r.str()? }),
            TAG_INPUT => {
                let n = r.u16()? as usize;
                let mut inputs = Vec::with_capacity(n);
                for _ in 0..n {
                    inputs.push(InputFrame {
                        seq: r.u32()?,
                        throttle: r.unit()?,
                        yaw: r.unit()?,
                        pitch: r.unit()?,
                        strafe: r.unit()?,
                        up: r.unit()?,
                        fire: r.bool()?,
                        // Необязательный байт: старые пакеты его не содержат.
                        boost: r.bool().unwrap_or(false),
                    });
                }
                Ok(Message::ClientInput { inputs })
            }
            TAG_SNAPSHOT | TAG_SNAPSHOT_V2 => {
                let compact = tag == TAG_SNAPSHOT_V2;
                let (tick, ack_input_seq, baseline, n) = if compact {
                    (
                        r.varint()?,
                        r.varint()? as u32,
                        r.bool()?,
                        r.varint()? as usize,
                    )
                } else {
                    (r.u64()?, r.u32()?, r.bool()?, r.u16()? as usize)
                };
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    if compact {
                        entities.push(EntityState {
                            id: r.varint()? as u32,
                            pos: r.vec3_qv()?,
                            vel: r.vec3_qv()?,
                            yaw: r.f32()?,
                            pitch: r.f32()?,
                        });
                    } else {
                        entities.push(EntityState {
                            id: r.u32()?,
                            pos: r.vec3_q()?,
                            vel: r.vec3_q()?,
                            yaw: r.f32()?,
                            pitch: r.f32()?,
                        });
                    }
                }
                Ok(Message::ServerSnapshot(Snapshot { tick, ack_input_seq, baseline, entities }))
            }
            TAG_ACK => Ok(Message::ServerAck { last_input_seq: r.u32()?, server_tick: r.u64()? }),
            TAG_BUILD => {
                let entity = r.u32()?;
                let action = match r.u8()? {
                    0 => BuildAction::Place,
                    1 => BuildAction::Remove,
                    2 => BuildAction::Paint,
                    other => return Err(WireError::BadTag(other)),
                };
                let coord = (r.i32()?, r.i32()?, r.i32()?);
                let color = Color(r.u8()?, r.u8()?, r.u8()?);
                Ok(Message::ClientBuild(BuildCommand { entity, action, coord, color }))
            }
            TAG_BUILD_RESULT => {
                let accepted = r.bool()?;
                let reason = match r.u8()? {
                    0 => BuildReason::Ok,
                    1 => BuildReason::Invalid,
                    2 => BuildReason::Disconnected,
                    3 => BuildReason::LimitReached,
                    other => return Err(WireError::BadTag(other)),
                };
                Ok(Message::ServerBuildResult { accepted, reason })
            }
            TAG_WELCOME => Ok(Message::ServerWelcome { entity: r.varint()? as NetId }),
            TAG_BUILD_EDIT => {
                let entity = r.varint()? as NetId;
                let author = r.varint()? as NetId;
                let action = match r.u8()? {
                    0 => BuildAction::Place,
                    1 => BuildAction::Remove,
                    2 => BuildAction::Paint,
                    other => return Err(WireError::BadTag(other)),
                };
                let coord = (r.i32()?, r.i32()?, r.i32()?);
                let color = Color(r.u8()?, r.u8()?, r.u8()?);
                let revision = r.varint()?;
                Ok(Message::ServerBuildEdit(BuildEdit { entity, author, action, coord, color, revision }))
            }
            TAG_BUILD_ACK => Ok(Message::ClientBuildAck {
                entity: r.varint()? as NetId,
                revision: r.varint()?,
            }),
            TAG_BUILD_GRANT => Ok(Message::ClientGrantBuild {
                entity: r.varint()? as NetId,
                player: r.varint()? as NetId,
            }),
            TAG_BUILD_ACCESS => Ok(Message::ServerBuildAccess {
                entity: r.varint()? as NetId,
                can_edit: r.bool()?,
            }),
            TAG_SHOT => Ok(Message::ServerShot(ShotEvent {
                attacker: r.varint()? as NetId,
                target: r.varint()? as NetId,
                hit: r.bool()?,
                damage: r.varint()? as u32,
                destroyed: r.bool()?,
            })),
            TAG_HULL => Ok(Message::ServerHull {
                entity: r.varint()? as NetId,
                shield: r.varint()? as u32,
                armor: r.varint()? as u32,
                destroyed: r.bool()?,
            }),
            TAG_DEATH => Ok(Message::ServerDeath {
                entity: r.varint()? as NetId,
                respawn_tick: r.varint()?,
            }),
            TAG_RESPAWN => Ok(Message::ServerRespawn {
                entity: r.varint()? as NetId,
                invuln_until: r.varint()?,
            }),
            TAG_CLIENT_CHARACTER => Ok(Message::ClientCharacter {
                entity: r.varint()? as NetId,
                pos: r.vec3_qv()?,
                yaw: r.f32()?,
                moving: r.bool()?,
                active: r.bool()?,
            }),
            TAG_SERVER_CHARACTER => Ok(Message::ServerCharacter {
                entity: r.varint()? as NetId,
                pos: r.vec3_qv()?,
                yaw: r.f32()?,
                moving: r.bool()?,
            }),
            TAG_SYSTEM => {
                let id = r.varint()? as u32;
                let name = r.str()?;
                let n = r.varint()? as usize;
                let mut bodies = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    bodies.push(SystemBodyDto {
                        id: r.varint()? as u32,
                        kind: r.u8()?,
                        surface: r.u8()?,
                        center: Vec3::new(r.f32()? as f64, r.f32()? as f64, r.f32()? as f64),
                        orbit: r.f32()?,
                        phase: r.f32()?,
                        omega: r.f32()?,
                        radius: r.f64()?,
                    });
                }
                Ok(Message::ServerSystem { id, name, bodies })
            }
            TAG_DIG => {
                let from_server = r.bool()?;
                let slot = r.u32()?;
                let dir = Vec3::new(r.f64()?, r.f64()?, r.f64()?);
                let cos_radius = r.f64()?;
                let depth = r.f64()?;
                if from_server {
                    Ok(Message::ServerTerrainDig { slot, dir, cos_radius, depth })
                } else {
                    Ok(Message::ClientDig { slot, dir, cos_radius, depth })
                }
            }
            TAG_MARKET_ORDER => Ok(Message::ClientMarket {
                side: r.u8()?,
                item: r.varint()? as u32,
                price: r.varint()?,
                qty: r.varint()?,
            }),
            TAG_MARKET_STATE => {
                let credits = r.varint()?;
                let ni = r.varint()? as usize;
                let mut items = Vec::with_capacity(ni);
                for _ in 0..ni {
                    items.push((r.varint()? as u32, r.varint()?));
                }
                let nb = r.varint()? as usize;
                let mut bids = Vec::with_capacity(nb);
                for _ in 0..nb {
                    bids.push((r.varint()? as u32, r.varint()?, r.varint()?));
                }
                let na = r.varint()? as usize;
                let mut asks = Vec::with_capacity(na);
                for _ in 0..na {
                    asks.push((r.varint()? as u32, r.varint()?, r.varint()?));
                }
                Ok(Message::ServerMarket { credits, items, bids, asks })
            }
            TAG_DIG_YIELD => {
                let item = r.varint()? as u32;
                let qty = r.varint()?;
                let layer = r.u8()?;
                let hole_depth = r.f64()?;
                Ok(Message::ServerDigYield { item, qty, layer, hole_depth })
            }
            TAG_PING => Ok(Message::ClientPing { nonce: r.u32()? }),
            TAG_PONG => Ok(Message::ServerPong { nonce: r.u32()? }),
            TAG_RECIPES => {
                let n = r.varint()? as usize;
                let mut recipes = Vec::with_capacity(n);
                for _ in 0..n {
                    let id = r.varint()? as u32;
                    let name = r.str()?;
                    let ni = r.varint()? as usize;
                    let mut inputs = Vec::with_capacity(ni);
                    for _ in 0..ni {
                        inputs.push((r.varint()? as u32, r.varint()?));
                    }
                    let oitem = r.varint()? as u32;
                    let oqty = r.varint()?;
                    let ticks = r.varint()?;
                    recipes.push((id, name, inputs, (oitem, oqty), ticks));
                }
                Ok(Message::ServerRecipes { recipes })
            }
            TAG_CRAFT => Ok(Message::ClientCraft { recipe: r.varint()? as u32 }),
            TAG_CRAFT_RESULT => {
                let ok = r.bool()?;
                let item = r.varint()? as u32;
                let qty = r.varint()?;
                let reason = r.str()?;
                Ok(Message::ServerCraftResult { ok, item, qty, reason })
            }
            TAG_TERRITORY => {
                let n = r.varint()? as usize;
                let mut territories = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    territories.push(TerritoryDto {
                        id: r.varint()? as u32,
                        owner: r.varint()? as u32,
                        status: r.u8()?,
                        center: r.vec3_qv()?,
                        radius: r.f64()?,
                        control_shield: r.varint()? as u32,
                        tax_percent: r.varint()? as u32,
                        collected_tax: r.varint()?,
                    });
                }
                Ok(Message::ServerTerritory { territories })
            }
            TAG_CAPTURE => Ok(Message::ClientCapture { id: r.varint()? as u32 }),
            other => Err(WireError::BadTag(other)),
        }
    }
}

pub use wire::{decode, encode, Reader, WireError, Writer};

/// Максимальный размер одного UDP-пакета (безопасно ниже MTU, чтобы не дробился).
pub const MAX_DATAGRAM: usize = 1200;

/// Упаковка сообщения в датаграмму: 4 байта длины (LE) + тело.
/// Нужна для потока байт (TCP) и для нескольких сообщений в одной датаграмме.
pub fn frame(msg: &Message) -> Vec<u8> {
    let body = encode(msg);
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Разбор одной датаграммы: возвращает (сообщение, сколько байт занято).
pub fn deframe(buf: &[u8]) -> Result<(Message, usize), WireError> {
    if buf.len() < 4 {
        return Err(WireError::UnexpectedEof);
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_DATAGRAM {
        return Err(WireError::TooLarge(len));
    }
    if buf.len() < 4 + len {
        return Err(WireError::UnexpectedEof);
    }
    let msg = decode(&buf[4..4 + len])?;
    Ok((msg, 4 + len))
}

/// Контрольная сумма состояния мира (FNV‑1a). Используется для проверки
/// детерминизма: одинаковые входы → одинаковый хеш на сервере и в тестах.
pub fn state_checksum(map: &BTreeMap<NetId, EntityState>) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |b: u64| {
        h ^= b;
        h = h.wrapping_mul(0x100000001b3);
    };
    for (id, e) in map {
        mix(*id as u64);
        for c in e.pos.to_wire() {
            mix(c as u32 as u64);
        }
        for c in e.vel.to_wire() {
            mix(c as u32 as u64);
        }
        mix(e.yaw.to_bits() as u64);
        mix(e.pitch.to_bits() as u64);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_roundtrip() {
        let order = Message::ClientMarket { side: 1, item: 4, price: 40, qty: 25 };
        assert_eq!(decode(&encode(&order)).unwrap(), order);
        let state = Message::ServerMarket {
            credits: 1500,
            items: vec![(1, 120), (4, 25)],
            bids: vec![(1, 30, 40)],
            asks: vec![(1, 25, 60), (4, 90, 10)],
        };
        assert_eq!(decode(&encode(&state)).unwrap(), state);
    }

    #[test]
    fn dig_yield_and_ping_roundtrip() {
        let y = Message::ServerDigYield { item: 3, qty: 7, layer: 2, hole_depth: 55.5 };
        assert_eq!(decode(&encode(&y)).unwrap(), y);
        let ping = Message::ClientPing { nonce: 12345 };
        assert_eq!(decode(&encode(&ping)).unwrap(), ping);
        let pong = Message::ServerPong { nonce: 12345 };
        assert_eq!(decode(&encode(&pong)).unwrap(), pong);
        let recipes = Message::ServerRecipes {
            recipes: vec![
                (1, "выплавка металла".into(), vec![(1, 2)], (4, 1), 5),
                (7, "сборка корабля".into(), vec![(8, 1), (9, 2), (7, 2)], (10, 1), 30),
            ],
        };
        assert_eq!(decode(&encode(&recipes)).unwrap(), recipes);
        let craft = Message::ClientCraft { recipe: 3 };
        assert_eq!(decode(&encode(&craft)).unwrap(), craft);
        let res = Message::ServerCraftResult { ok: false, item: 0, qty: 0, reason: "не хватает входов".into() };
        assert_eq!(decode(&encode(&res)).unwrap(), res);
    }

    #[test]
    fn territory_roundtrip() {
        let t = Message::ServerTerritory {
            territories: vec![
                TerritoryDto {
                    id: 1,
                    owner: 7,
                    status: 1,
                    center: Vec3::new(1234.5, -6789.25, 42.0),
                    radius: 2500.0,
                    control_shield: 80,
                    tax_percent: 5,
                    collected_tax: 12345,
                },
                TerritoryDto {
                    id: 2,
                    owner: 0,
                    status: 0,
                    center: Vec3::new(-100.0, 200.0, 300.0),
                    radius: 500.0,
                    control_shield: 100,
                    tax_percent: 0,
                    collected_tax: 0,
                },
            ],
        };
        assert_eq!(decode(&encode(&t)).unwrap(), t);
        let cap = Message::ClientCapture { id: 1 };
        assert_eq!(decode(&encode(&cap)).unwrap(), cap);
    }

    #[test]
    fn dig_roundtrip() {
        let req = Message::ClientDig {
            slot: 3,
            dir: Vec3::new(0.1, -0.9, 0.42),
            cos_radius: 0.9998,
            depth: 12.5,
        };
        assert_eq!(decode(&encode(&req)).unwrap(), req);
        let applied = Message::ServerTerrainDig {
            slot: 3,
            dir: Vec3::new(0.1, -0.9, 0.42),
            cos_radius: 0.9998,
            depth: 12.5,
        };
        assert_eq!(decode(&encode(&applied)).unwrap(), applied);
    }

    #[test]
    fn roundtrip_quantized_position() {        let v = Vec3::new(1234.5678, -98765.4321, 0.0625);
        let w = v.to_wire();
        let back = Vec3::from_wire(w);
        // Точность — 1 мм.
        assert!((back.x - v.x).abs() < 1e-3);
        assert!((back.y - v.y).abs() < 1e-3);
        assert!((back.z - v.z).abs() < 1e-3);
    }

    #[test]
    fn quantized_position_covers_planet_radius() {
        // Планета радиусом 500 км: позиции у поверхности должны выражаться
        // проводом без переполнения (иначе сущность «уезжает» с поверхности).
        let v = Vec3::new(500_000.0, -500_000.0, 123_456.789);
        let back = Vec3::from_wire(v.to_wire());
        assert!((back.x - v.x).abs() < 1e-2, "x переполнился: {}", back.x);
        assert!((back.y - v.y).abs() < 1e-2, "y переполнился: {}", back.y);
    }

    #[test]
    fn hello_roundtrip() {
        let m = Message::ClientHello { protocol: 1, name: "pilot".into() };
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn input_roundtrip() {
        let m = Message::ClientInput {
            inputs: vec![
                InputFrame { seq: 10, throttle: 1.0, yaw: -0.5, pitch: 0.25, strafe: -0.3, up: 0.4, fire: true, ..Default::default() },
                InputFrame::neutral(11),
            ],
        };
        let bytes = encode(&m);
        let back = decode(&bytes).unwrap();
        match back {
            Message::ClientInput { inputs } => {
                assert_eq!(inputs.len(), 2);
                assert_eq!(inputs[0].seq, 10);
                assert!(inputs[0].fire);
                assert!((inputs[0].yaw + 0.5).abs() < 1e-3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn snapshot_roundtrip_and_delta_size() {
        let snap = Snapshot {
            tick: 42,
            ack_input_seq: 7,
            baseline: false,
            entities: vec![EntityState {
                id: 1,
                pos: Vec3::new(100.0, 200.0, 300.0),
                vel: Vec3::new(1.0, 2.0, 3.0),
                yaw: 0.5,
                pitch: -0.25,
            }],
        };
        let bytes = encode(&Message::ServerSnapshot(snap.clone()));
        assert_eq!(decode(&bytes).unwrap(), Message::ServerSnapshot(snap));
        // Одна изменившаяся сущность должна быть компактной.
        assert!(bytes.len() < 60, "snapshot too large: {}", bytes.len());
    }

    #[test]
    fn framing_roundtrip() {
        let m = Message::ServerAck { last_input_seq: 99, server_tick: 12345 };
        let dg = frame(&m);
        let (back, used) = deframe(&dg).unwrap();
        assert_eq!(back, m);
        assert_eq!(used, dg.len());
    }

    #[test]
    fn welcome_roundtrip() {
        for entity in [1u32, 7, 42, 1000] {
            let m = Message::ServerWelcome { entity };
            let bytes = encode(&m);
            assert_eq!(decode(&bytes).unwrap(), m);
            // Приветствие должно быть компактным (varint).
            assert!(bytes.len() <= 4, "welcome too large: {}", bytes.len());
        }
    }

    #[test]
    fn build_edit_roundtrip() {
        let m = Message::ServerBuildEdit(BuildEdit {
            entity: 3,
            author: 5,
            action: BuildAction::Paint,
            coord: (-4, 7, 0),
            color: Color(11, 22, 33),
            revision: 128,
        });
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn shot_and_hull_roundtrip() {
        let shot = Message::ServerShot(ShotEvent {
            attacker: 1,
            target: 2,
            hit: true,
            damage: 42,
            destroyed: false,
        });
        assert_eq!(decode(&encode(&shot)).unwrap(), shot);

        let miss = Message::ServerShot(ShotEvent {
            attacker: 3,
            target: 0,
            hit: false,
            damage: 0,
            destroyed: false,
        });
        assert_eq!(decode(&encode(&miss)).unwrap(), miss);

        let hull = Message::ServerHull { entity: 7, shield: 30, armor: 80, destroyed: false };
        assert_eq!(decode(&encode(&hull)).unwrap(), hull);
    }

    #[test]
    fn death_and_respawn_roundtrip() {
        let death = Message::ServerDeath { entity: 5, respawn_tick: 4200 };
        assert_eq!(decode(&encode(&death)).unwrap(), death);
        let respawn = Message::ServerRespawn { entity: 5, invuln_until: 4260 };
        assert_eq!(decode(&encode(&respawn)).unwrap(), respawn);
    }

    #[test]
    fn system_roundtrip() {
        let m = Message::ServerSystem {
            id: 7,
            name: "Alra-7".into(),
            bodies: vec![
                SystemBodyDto {
                    id: 0,
                    kind: 0,
                    surface: 4,
                    center: Vec3::new(0.0, 0.0, 0.0),
                    orbit: 0.0,
                    phase: 0.0,
                    omega: 0.0,
                    radius: 500.0,
                },
                SystemBodyDto {
                    id: 1,
                    kind: 1,
                    surface: 2,
                    center: Vec3::new(100.0, 0.0, 0.0),
                    orbit: 2500.0,
                    phase: 0.5,
                    omega: 0.02,
                    radius: 120.5,
                },
            ],
        };
        assert_eq!(decode(&encode(&m)).unwrap(), m);
    }

    #[test]
    fn character_pose_roundtrip() {
        let c = Message::ClientCharacter {
            entity: 2,
            pos: Vec3::new(12.5, -3.25, 40.0),
            yaw: 1.25,
            moving: true,
            active: true,
        };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        let s = Message::ServerCharacter {
            entity: 2,
            pos: Vec3::new(12.5, -3.25, 40.0),
            yaw: 1.25,
            moving: false,
        };
        assert_eq!(decode(&encode(&s)).unwrap(), s);
    }

    #[test]
    fn build_ack_grant_access_roundtrip() {        let ack = Message::ClientBuildAck { entity: 2, revision: 300 };
        assert_eq!(decode(&encode(&ack)).unwrap(), ack);
        let grant = Message::ClientGrantBuild { entity: 1, player: 9 };
        assert_eq!(decode(&encode(&grant)).unwrap(), grant);
        let access = Message::ServerBuildAccess { entity: 4, can_edit: false };
        assert_eq!(decode(&encode(&access)).unwrap(), access);
    }

    #[test]
    fn build_command_roundtrip() {
        let m = Message::ClientBuild(BuildCommand {
            entity: 7,
            action: BuildAction::Paint,
            coord: (-3, 0, 12),
            color: Color(10, 20, 30),
        });
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn all_build_actions_roundtrip() {
        for action in [BuildAction::Place, BuildAction::Remove, BuildAction::Paint] {
            let m = Message::ClientBuild(BuildCommand {
                entity: 1,
                action,
                coord: (1, 2, 3),
                color: Color(255, 128, 0),
            });
            assert_eq!(decode(&encode(&m)).unwrap(), m);
        }
    }

    #[test]
    fn build_result_roundtrip() {
        for reason in [
            BuildReason::Ok,
            BuildReason::Invalid,
            BuildReason::Disconnected,
            BuildReason::LimitReached,
        ] {
            let m = Message::ServerBuildResult { accepted: reason == BuildReason::Ok, reason };
            assert_eq!(decode(&encode(&m)).unwrap(), m);
        }
    }

    #[test]
    fn deframe_rejects_truncated() {
        let m = Message::ClientHello { protocol: 1, name: "abcdefgh".into() };
        let mut dg = frame(&m);
        dg.truncate(dg.len() - 2);
        assert!(matches!(deframe(&dg), Err(WireError::UnexpectedEof)));
    }

    #[test]
    fn compact_snapshot_is_smaller_and_roundtrips() {
        // Небольшие координаты — типичный случай в локальной зоне.
        let snap = Snapshot {
            tick: 1000,
            ack_input_seq: 25,
            baseline: false,
            entities: vec![
                EntityState {
                    id: 7,
                    pos: Vec3::new(120.5, -30.25, 8.0),
                    vel: Vec3::new(1.0, 0.0, -2.0),
                    yaw: 0.5,
                    pitch: -0.25,
                },
                EntityState {
                    id: 8,
                    pos: Vec3::new(-300.0, 12.0, 40.0),
                    vel: Vec3::new(0.0, 3.0, 1.0),
                    yaw: -1.0,
                    pitch: 0.1,
                },
            ],
        };
        let bytes = encode(&Message::ServerSnapshot(snap.clone()));
        // Varint-вариант должен быть заметно компактнее «широкого».
        let wide = 8 /*tick*/ + 4 /*ack*/ + 1 + 2 + 2 * (4 + 12 + 12 + 4 + 4);
        assert!(bytes.len() < wide, "compact={} wide={}", bytes.len(), wide);
        assert_eq!(decode(&bytes).unwrap(), Message::ServerSnapshot(snap));
    }

    #[test]
    fn varint_and_zigzag_roundtrip() {
        let values: [i64; 9] = [0, 1, -1, 63, 64, -64, 1_000_000, -1_000_000, i32::MAX as i64];
        let mut w = Writer::new();
        for v in values {
            w.svarint(v);
        }
        let mut r = Reader::new(&w.buf);
        for v in values {
            assert_eq!(r.svarint().unwrap(), v);
        }
    }

    #[test]
    fn varint_is_compact_for_small_numbers() {
        let mut w = Writer::new();
        w.varint(5);
        w.varint(200);
        w.varint(100_000);
        // 5 → 1 байт, 200 → 2 байта, 100000 → 3 байта.
        assert_eq!(w.buf.len(), 6);
    }
}
