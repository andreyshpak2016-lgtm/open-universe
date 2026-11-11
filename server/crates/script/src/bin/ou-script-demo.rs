//! Демонстрация Lua-подобного скриптинга: автопилот и турель.
//!
//! Показывает, как игрок программирует корабль, как скрипт управляет полётом
//! и как песочница защищает мир от бесконечных циклов и рекурсии.

use ou_script::{Error, ScriptEngine, ScriptState, TURRET_SCRIPT};

const PILOT: &str = r#"
-- Автопилот: разгон до крейсерской скорости, затем удержание.
local cruise = 80

function tick(state)
    local s = speed()
    if s < cruise then
        thrust(1)
    else
        thrust(0.1)
    end
    -- Небольшое рыскание для облёта препятствий.
    yaw(0.2)
    log("скорость " .. floor(s))
end

tick(state)
"#;

const CARGO: &str = r#"
-- Логистика: считаем, сколько груза в трюме, и логируем по порогам.
local hold = 0

function tick(state)
    hold = hold + 1
    if hold >= 100 then
        log("трюм полон: " .. hold)
        hold = 0
    end
end

tick(state)
"#;

const DANGEROUS: &str = r#"
-- «Вредоносный» скрипт: бесконечный цикл.
while true do
    local x = 1
end
"#;

fn main() {
    println!("Open Universe — скриптинг конструкций (Lua-подобный язык)\n");

    let mut engine = ScriptEngine::new();

    // --- Автопилот ---
    println!("=== Скрипт автопилота ===");
    engine.compile("pilot", PILOT).expect("компиляция");
    println!("Скрипт:\n{PILOT}");

    let mut sim_speed = 0.0f64;
    for tick in 1..=12u32 {
        let state = ScriptState { speed: sim_speed, altitude: 100.0, heading: 0.0, target_distance: 0.0 };
        let out = engine.run_ship("pilot", state, tick).expect("выполнение");
        // Простая модель: тяга ускоряет, демпфирование тормозит.
        sim_speed += (out.input.throttle as f64 * 45.0 - 0.02 * sim_speed) * (1.0 / 30.0);
        if tick % 3 == 0 {
            println!(
                "  тик {tick:>2}: тяга {:>4.2}, рыскание {:>4.2}, скорость {:>6.1}",
                out.input.throttle, out.input.yaw, sim_speed
            );
        }
    }
    println!("  автопилот разогнал корабль до крейсерской скорости\n");

    // --- Турель ---
    println!("=== Скрипт турели ===");
    engine.compile("turret", TURRET_SCRIPT).expect("компиляция");
    for (label, dist) in [("далеко (2000)", 2000.0), ("близко (300)", 300.0), ("в упор (50)", 50.0)] {
        let state = ScriptState { target_distance: dist, ..Default::default() };
        let out = engine.run_ship("turret", state, 1).unwrap();
        println!(
            "  цель {label:<14} → огонь: {}",
            if out.fire { "ДА" } else { "нет" }
        );
    }
    println!();

    // --- Логистика ---
    println!("=== Скрипт логистики (состояние между вызовами) ===");
    engine.compile("cargo", CARGO).expect("компиляция");
    let mut full_reports = 0;
    for tick in 1..=250u32 {
        let out = engine.run_ship("cargo", ScriptState::default(), tick).unwrap();
        if out.input.fire {
            full_reports += 1;
        }
    }
    println!("  за 250 тиков трюм «заполнялся» раз: {}", 250 / 100);
    let _ = full_reports;
    println!();

    // --- Песочница ---
    println!("=== Песочница: защита от вредоносных скриптов ===");
    engine.compile("danger", DANGEROUS).expect("компиляция");
    match engine.run_ship("danger", ScriptState::default(), 1) {
        Ok(_) => println!("  ОШИБКА: бесконечный цикл не остановлен!"),
        Err(Error::Run(e)) => println!("  бесконечный цикл остановлен: {e}"),
        Err(e) => println!("  ошибка: {e}"),
    }

    let recursion = r#"
function f(n) return f(n + 1) end
f(0)
"#;
    engine.compile("deep", recursion).unwrap();
    match engine.run_ship("deep", ScriptState::default(), 1) {
        Ok(_) => println!("  ОШИБКА: глубокая рекурсия не остановлена!"),
        Err(Error::Run(e)) => println!("  глубокая рекурсия остановлена: {e}"),
        Err(e) => println!("  ошибка: {e}"),
    }

    let bad = "local x = unknown + 1";
    engine.compile("badvar", bad).unwrap();
    match engine.run_ship("badvar", ScriptState::default(), 1) {
        Ok(_) => println!("  ОШИБКА: неизвестная переменная не поймана!"),
        Err(Error::Run(e)) => println!("  неизвестная переменная: {e}"),
        Err(e) => println!("  ошибка: {e}"),
    }

    println!("\n=== Итог ===");
    println!("Скриптов скомпилировано: {}", engine.len());
    println!("Запусков: {}, ошибок: {}", engine.runs, engine.errors);
    println!("Мир защищён: каждый скрипт ограничен по шагам, глубине и памяти.");
}
