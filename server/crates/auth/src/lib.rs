//! # ou-auth
//!
//! Микросервис аутентификации Open Universe (один из сервисов шарда:
//! **auth**, мир, чат, рынок).
//!
//! ## Зачем отдельный сервис
//! В едином шарде вход игрока — это отдельная стадия: выдать **билет сессии**
//! (ticket), который затем предъявляется шарду-владельцу зоны. Так мир может
//! жить в нескольких процессах, а вход проверяется централизованно и
//! детерминированно.
//!
//! ## Что здесь есть
//! * [`AuthService`] — выпуск и проверка билетов сессии.
//! * Билет привязан к имени (позывному), имеет срок жизни (в тиках) и
//!   детерминированную подпись — сервер шарда проверяет его без обращения к БД.
//! * Список активных сессий (кто в сети) — для мониторинга и администрирования.
//!
//! ## Детерминизм
//! Подпись билета — чистая функция от секрета, имени и срока, без случайности.
//! Любой узел с тем же секретом проверит билет одинаково.

use std::collections::BTreeMap;

/// Секрет подписи билетов (у всех узлов один — из конфигурации/окружения).
pub type Secret = u64;

/// Билет сессии: предъявляется шарду для входа.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    /// Идентификатор сессии (выдаётся сервисом, монотонный).
    pub session: u64,
    /// Имя (позывной) игрока, хешированное в 64 бита (детерминированно).
    pub name_hash: u64,
    /// Подпись: `hash(secret, session, name_hash, exp)`.
    pub signature: u64,
    /// Тик, до которого билет действителен.
    pub expires_tick: u64,
}

/// Служебный FNV-1a + SplitMix64 — те же, что в остальном ядре.
fn mix(mut h: u64, v: u64) -> u64 {
    h ^= v;
    h = h.wrapping_mul(0x100000001b3);
    h
}

/// Финальное лавинное перемешивание.
fn finalize(z: u64) -> u64 {
    let mut z = z;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Хеш имени (детерминированный; сервер сверяет с ним подпись).
pub fn hash_name(name: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.as_bytes() {
        h = mix(h, *b as u64);
    }
    finalize(h)
}

/// Подпись билета — чистая функция секрета и полей билета.
fn sign(secret: Secret, session: u64, name_hash: u64, exp: u64) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h = mix(h, secret);
    h = mix(h, session);
    h = mix(h, name_hash);
    h = mix(h, exp);
    finalize(h)
}

/// Сервис аутентификации: выдаёт и проверяет билеты.
#[derive(Clone, Debug)]
pub struct AuthService {
    secret: Secret,
    next_session: u64,
    /// Активные сессии: session → (имя, срок). Для мониторинга и выхода.
    sessions: BTreeMap<u64, (String, u64)>,
}

impl AuthService {
    pub fn new(secret: Secret) -> Self {
        AuthService { secret, next_session: 1, sessions: BTreeMap::new() }
    }

    /// Выдать билет на имя `name`, действующий до тика `expires_tick`.
    ///
    /// Если у имени уже есть активная сессия — она закрывается (новый вход
    /// вытесняет старый: один игрок — одна сессия).
    pub fn issue(&mut self, name: &str, expires_tick: u64) -> Ticket {
        // Вытесняем прежние сессии этого игрока.
        let stale: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, (n, _))| n == name)
            .map(|(s, _)| *s)
            .collect();
        for s in stale {
            self.sessions.remove(&s);
        }
        let session = self.next_session;
        self.next_session += 1;
        let name_hash = hash_name(name);
        let signature = sign(self.secret, session, name_hash, expires_tick);
        self.sessions.insert(session, (name.to_string(), expires_tick));
        Ticket { session, name_hash, signature, expires_tick }
    }

    /// Проверить билет на тике `now`: подпись верна и срок не истёк.
    pub fn verify(&self, t: &Ticket, now: u64) -> bool {
        if now > t.expires_tick {
            return false;
        }
        let expect = sign(self.secret, t.session, t.name_hash, t.expires_tick);
        expect == t.signature
    }

    /// Продлить активную сессию (heartbeat игрока): продлевает срок и выдаёт
    /// заново подписанный билет с новой датой. `None` — сессии нет.
    pub fn renew(&mut self, session: u64, expires_tick: u64) -> Option<Ticket> {
        let name_hash = {
            let (name, _) = self.sessions.get(&session)?;
            hash_name(name)
        };
        let signature = sign(self.secret, session, name_hash, expires_tick);
        if let Some(e) = self.sessions.get_mut(&session) {
            e.1 = expires_tick;
        }
        Some(Ticket { session, name_hash, signature, expires_tick })
    }

    /// Игрок вышел: закрыть сессию.
    pub fn logout(&mut self, session: u64) {
        self.sessions.remove(&session);
    }

    /// Сколько игроков сейчас в сети.
    pub fn online(&self) -> usize {
        self.sessions.len()
    }

    /// Имя активной сессии.
    pub fn name_of(&self, session: u64) -> Option<&str> {
        self.sessions.get(&session).map(|(n, _)| n.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_ticket_verifies() {
        let mut a = AuthService::new(0x5EED);
        let t = a.issue("pilot", 1000);
        assert!(a.verify(&t, 10), "свежий билет действителен");
        assert!(a.verify(&t, 1000), "на границе срока — действителен");
        assert!(!a.verify(&t, 1001), "просроченный билет отвергается");
    }

    #[test]
    fn wrong_secret_rejects() {
        let mut a = AuthService::new(0x5EED);
        let t = a.issue("pilot", 1000);
        let b = AuthService::new(0xBEEF);
        assert!(!b.verify(&t, 10), "чужой секрет не принимает билет");
    }

    #[test]
    fn duplicate_name_evicts_old_session() {
        let mut a = AuthService::new(1);
        let t1 = a.issue("pilot", 100);
        let t2 = a.issue("pilot", 100);
        assert_ne!(t1.session, t2.session);
        assert_eq!(a.online(), 1, "один игрок — одна сессия");
        assert!(a.name_of(t2.session).is_some());
    }

    #[test]
    fn logout_and_renew() {
        let mut a = AuthService::new(1);
        let t = a.issue("pilot", 100);
        let t2 = a.renew(t.session, 200).expect("сессия есть");
        assert!(a.verify(&t2, 150), "после продления билет живёт дольше");
        assert!(!a.verify(&t, 150), "старый билет со старым сроком уже недействителен");
        a.logout(t.session);
        assert_eq!(a.online(), 0);
        assert!(a.renew(t.session, 300).is_none(), "сессии больше нет");
    }
}
