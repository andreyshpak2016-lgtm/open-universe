//! # ou-net
//!
//! UDP-транспорт Open Universe поверх бинарного протокола из `ou-protocol`.
//!
//! Задачи транспорта:
//! * разделять потоки байтов на сообщения (framing — уже в протоколе);
//! * отбрасывать повреждённые и устаревшие дейтаграммы;
//! * защищать от подделки источника на уровне UDP (лёгкий «session token»);
//! * агрегировать несколько сообщений в одну дейтаграмму (batching).
//!
//! Детерминизм ядру не нужен: порядок доставки пакетов не влияет на симуляцию,
//! потому что привязка идёт к номерам кадров (`seq`) и тиков.

use ou_protocol::{deframe, frame, Message, MAX_DATAGRAM};
use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};

/// Случайный токен сессии, который клиент прикладывает к каждому пакету.
///
/// Это не заменяет криптографию, но отсекает случайный мусор и наивную
/// подмену адреса на этапе handshake.
pub type SessionToken = u64;

/// Ошибки транспорта.
#[derive(Debug)]
pub enum NetError {
    /// Ошибка ввода/вывода сокета.
    Io(std::io::Error),
    /// Повреждённый или обрезанный пакет.
    Decode(ou_protocol::WireError),
    /// Пакет не принадлежит этой сессии.
    BadToken,
    /// Пакет не помещается в дельту.
    TooLarge,
    /// Очередь пуста.
    Empty,
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Io(e) => write!(f, "io: {e}"),
            NetError::Decode(e) => write!(f, "decode: {e:?}"),
            NetError::BadToken => write!(f, "неверный токен сессии"),
            NetError::TooLarge => write!(f, "пакет слишком велик"),
            NetError::Empty => write!(f, "очередь пуста"),
        }
    }
}

impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        NetError::Io(e)
    }
}

impl From<ou_protocol::WireError> for NetError {
    fn from(e: ou_protocol::WireError) -> Self {
        NetError::Decode(e)
    }
}

/// Пакет на проводе: 8 байт токена + тело сообщения (с framing).
///
/// Формат: `[token: u64 LE][len: u32 LE][body...]`
pub fn encode_packet(token: SessionToken, msg: &Message) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + 64);
    out.extend_from_slice(&token.to_le_bytes());
    out.extend_from_slice(&frame(msg));
    out
}

/// Разбор пакета с проверкой токена.
pub fn decode_packet(token: SessionToken, buf: &[u8]) -> Result<Message, NetError> {
    if buf.len() < 8 {
        return Err(NetError::Decode(ou_protocol::WireError::UnexpectedEof));
    }
    let got = u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ]);
    if got != token {
        return Err(NetError::BadToken);
    }
    let (msg, _used) = deframe(&buf[8..])?;
    Ok(msg)
}

/// Сокет-обёртка с исходящей очередью и буфером приёма.
pub struct UdpTransport {
    socket: UdpSocket,
    /// Исходящие пакеты: (адрес, байты). Позволяет агрегировать и слать пачкой.
    outbox: VecDeque<(SocketAddr, Vec<u8>)>,
    /// Буфер приёма фиксированного размера — без аллокаций на каждый пакет.
    recv_buf: Vec<u8>,
}

impl UdpTransport {
    /// Привязать сокет к адресу (по умолчанию — неблокирующий).
    pub fn bind(addr: &str) -> Result<Self, NetError> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(UdpTransport {
            socket,
            outbox: VecDeque::new(),
            recv_buf: vec![0u8; MAX_DATAGRAM + 16],
        })
    }

    /// Обернуть существующий сокет (например, уже подключённый клиентский).
    /// Режим блокировки не меняется — управляйте им через [`UdpTransport::set_nonblocking`].
    pub fn from_socket(socket: UdpSocket) -> Result<Self, NetError> {
        Ok(UdpTransport {
            socket,
            outbox: VecDeque::new(),
            recv_buf: vec![0u8; MAX_DATAGRAM + 16],
        })
    }

    /// Переключить режим блокировки сокета.
    pub fn set_nonblocking(&self, on: bool) -> Result<(), NetError> {
        self.socket.set_nonblocking(on)?;
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetError> {
        Ok(self.socket.local_addr()?)
    }

    /// Поставить пакет в очередь на отправку.
    pub fn send_to(&mut self, token: SessionToken, addr: SocketAddr, msg: &Message) -> Result<(), NetError> {
        let bytes = encode_packet(token, msg);
        if bytes.len() > MAX_DATAGRAM + 16 {
            return Err(NetError::TooLarge);
        }
        self.outbox.push_back((addr, bytes));
        Ok(())
    }

    /// Отправить всё, что накопилось в очереди. Возвращает число отправленных.
    pub fn flush(&mut self) -> Result<usize, NetError> {
        let mut sent = 0;
        while let Some((addr, bytes)) = self.outbox.pop_front() {
            self.socket.send_to(&bytes, addr)?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Принять один пакет (неблокирующе).
    pub fn recv(&mut self) -> Result<Option<(SocketAddr, Vec<u8>)>, NetError> {
        match self.socket.recv_from(&mut self.recv_buf) {
            Ok((n, from)) => Ok(Some((from, self.recv_buf[..n].to_vec()))),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(NetError::Io(e)),
        }
    }

    /// Принять один пакет и сразу разобрать по протоколу.
    pub fn recv_message(&mut self, token: SessionToken) -> Result<Option<(SocketAddr, Message)>, NetError> {
        match self.recv()? {
            Some((from, bytes)) => match decode_packet(token, &bytes) {
                Ok(msg) => Ok(Some((from, msg))),
                // Повреждённый/чужой пакет молча отбрасываем — это UDP.
                Err(NetError::BadToken) | Err(NetError::Decode(_)) => Ok(None),
                Err(e) => Err(e),
            },
            None => Ok(None),
        }
    }
}

/// Простой детерминированный генератор токена сессии (на основе времени).
pub fn new_session_token(seed: u64) -> SessionToken {
    let mut x = seed ^ 0xDEADBEEFCAFEF00D;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51AFD7ED558CCD);
    x ^= x >> 33;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use ou_protocol::{InputFrame, Snapshot, Vec3};

    #[test]
    fn packet_roundtrip_with_token() {
        let token = new_session_token(7);
        let msg = Message::ClientHello { protocol: 1, name: "pilot".into() };
        let bytes = encode_packet(token, &msg);
        assert_eq!(decode_packet(token, &bytes).unwrap(), msg);
    }

    #[test]
    fn wrong_token_is_rejected() {
        let msg = Message::ServerAck { last_input_seq: 1, server_tick: 2 };
        let bytes = encode_packet(111, &msg);
        let err = decode_packet(222, &bytes).unwrap_err();
        assert!(matches!(err, NetError::BadToken));
    }

    #[test]
    fn corrupted_body_is_rejected() {
        let token = 42;
        let msg = Message::ClientInput { inputs: vec![InputFrame::neutral(1)] };
        let mut bytes = encode_packet(token, &msg);
        bytes.truncate(bytes.len() - 1); // обрезаем тело
        let err = decode_packet(token, &bytes).unwrap_err();
        assert!(matches!(err, NetError::Decode(_)));
    }

    #[test]
    fn token_generation_is_deterministic_and_seed_dependent() {
        assert_eq!(new_session_token(5), new_session_token(5));
        assert_ne!(new_session_token(5), new_session_token(6));
    }

    #[test]
    fn real_udp_loopback_delivers_message() {
        // Настоящий обмен по localhost: клиент -> сервер.
        // Серверный сокет — блокирующий с таймаутом, чтобы тест не «крутился».
        let server_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        server_sock
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let server_addr = server_sock.local_addr().unwrap();
        let mut server = UdpTransport::from_socket(server_sock).unwrap();

        let client_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut client = UdpTransport::from_socket(client_sock).unwrap();

        let token = new_session_token(99);
        let snap = Snapshot {
            tick: 7,
            ack_input_seq: 3,
            baseline: true,
            entities: vec![ou_protocol::EntityState {
                id: 1,
                pos: Vec3::new(1.0, 2.0, 3.0),
                vel: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            }],
        };
        let msg = Message::ServerSnapshot(snap.clone());
        client.send_to(token, server_addr, &msg).unwrap();
        assert_eq!(client.flush().unwrap(), 1);

        let (from, bytes) = server.recv().unwrap().expect("пакет не пришёл");
        let decoded = decode_packet(token, &bytes).unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(from.port(), client.local_addr().unwrap().port());
    }
}
