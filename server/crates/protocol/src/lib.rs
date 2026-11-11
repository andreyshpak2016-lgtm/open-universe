//! # ou-protocol
//!
//! Бинарный сетевой протокол Open Universe (клиент ↔ сервер).
//!
//! Принципы:
//! * **Детерминизм.** Все вычисления — целочисленные или базовые IEEE‑754
//!   операции (`+ - * /`), без платформо‑зависимых `libm`‑функций.
//! * **Квантование.** Позиции передаются как `i32` с масштабом
//!   [`POS_SCALE`] (0.1 мм), что даёт диапазон ±214 км вокруг начала отсчёта.
//!   В паре с floating origin этого достаточно для локальной зоны.
//! * **Дельта‑кодирование.** Снимок содержит только изменившиеся сущности.
//! * **Единый контракт.** Одна схема — общая для C++ (UE5) и Rust.

use std::collections::BTreeMap;

/// Масштаб позиции: 1 единица провода = 1e-4 игровых единиц (0.1 мм).
pub const POS_SCALE: f64 = 10_000.0;
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputFrame {
    /// Монотонный номер кадра (используется для реконсиляции).
    pub seq: u32,
    /// Тяга вперёд, −1..1.
    pub throttle: f32,
    /// Поворот вокруг вертикальной оси, −1..1.
    pub yaw: f32,
    /// Тангаж, −1..1.
    pub pitch: f32,
    /// Огонь.
    pub fire: bool,
}

impl InputFrame {
    pub fn neutral(seq: u32) -> Self {
        InputFrame { seq, throttle: 0.0, yaw: 0.0, pitch: 0.0, fire: false }
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
    /// Снимок состояния с сервера.
    ServerSnapshot(Snapshot),
    /// Подтверждение: какой последний ввод сервер применил.
    ServerAck { last_input_seq: u32, server_tick: u64 },
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
                    w.bool(i.fire);
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
                        fire: r.bool()?,
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
    fn roundtrip_quantized_position() {
        let v = Vec3::new(1234.5678, -98765.4321, 0.0625);
        let w = v.to_wire();
        let back = Vec3::from_wire(w);
        // Точность — 0.1 мм.
        assert!((back.x - v.x).abs() < 1e-4);
        assert!((back.y - v.y).abs() < 1e-4);
        assert!((back.z - v.z).abs() < 1e-4);
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
                InputFrame { seq: 10, throttle: 1.0, yaw: -0.5, pitch: 0.25, fire: true },
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
