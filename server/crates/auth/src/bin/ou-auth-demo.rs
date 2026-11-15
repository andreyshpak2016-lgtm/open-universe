//! Демонстрация микросервиса аутентификации Open Universe.
//!
//! Показывает полный цикл: выдача билета сессии, проверка на «шарде», выход.
//! Запуск: `cargo run -p ou-auth --bin ou-auth-demo`

use ou_auth::AuthService;

fn main() {
    println!("Open Universe — микросервис аутентификации (auth)\n");

    let secret = 0x5EED_1234_5678_9ABC;
    let mut auth = AuthService::new(secret);

    // Игрок входит: сервис выдаёт билет на 10 секунд (300 тиков @ 30 Гц).
    let ticket = auth.issue("TestPilot", 300);
    println!(
        "выдан билет: сессия {}, истекает на тике {}",
        ticket.session, ticket.expires_tick
    );
    println!("  подпись: {:#018x}", ticket.signature);

    // Шард проверяет билет (знает секрет).
    println!(
        "  проверка на шарде (tick 10): {}",
        if auth.verify(&ticket, 10) { "ок" } else { "отказ" }
    );
    println!(
        "  проверка после срока (tick 400): {}",
        if auth.verify(&ticket, 400) { "ок" } else { "отказ — просрочен" }
    );

    // Один игрок — одна сессия: повторный вход вытесняет старую.
    let t2 = auth.issue("TestPilot", 600);
    println!("\nповторный вход: новая сессия {}, в сети {}", t2.session, auth.online());

    // Heartbeat продлевает сессию.
    let t2 = auth.renew(t2.session, 900).expect("сессия активна");
    println!("сердцебиение: сессия продлена до тика 900 (подпись {:#018x})", t2.signature);

    // Выход.
    auth.logout(t2.session);
    println!("выход: в сети {}", auth.online());

    println!("\nитог: сервис auth готов; билеты детерминированы и проверяются без БД.");
}
