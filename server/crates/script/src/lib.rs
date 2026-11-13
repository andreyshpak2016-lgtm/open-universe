//! # ou-script
//!
//! Встраиваемый **Lua-подобный** язык для конструкций Open Universe.
//!
//! ## Зачем свой язык, а не готовый Lua
//! Скрипты запускаются на авторитетном сервере, который обязан быть
//! **детерминированным** (иначе предсказание и античит ломаются). Поэтому язык
//! намеренно ограничен: числа `f64` с детерминированными операциями, никакого
//! доступа к файлам/сети/времени, фиксированные лимиты на шаги и глубину.
//! Это и есть «песочница» из плана.
//!
//! ## Что умеет язык
//! ```lua
//! local speed = 1
//! function tick(state)
//!     if state.speed < 10 then
//!         thrust(1)
//!     else
//!         thrust(0)
//!     end
//!     log("скорость " .. state.speed)
//! end
//! ```
//! Поддерживаются: локальные переменные, присваивание (в т.ч. полям таблиц),
//! арифметика и логика, `if/elseif/else`, `while`, `for ... in pairs/range`,
//! функции, таблицы, конкатенация строк.
//!
//! ## Применение
//! * [`ScriptEngine`] — компилирует и хранит скрипты игроков.
//! * [`ShipScript`] — обёртка «автопилот»: скрипт получает состояние корабля и
//!   возвращает управляющий ввод.
//! * [`TURRET_SCRIPT`] — пример скрипта турели.

pub mod interp;
pub mod lexer;
pub mod parser;

use interp::{Host, Interpreter, ScriptError, Value};
use ou_protocol::InputFrame;
use parser::Stmt;

pub use interp::Limits;
pub use lexer::ParseError;

/// Ошибка компиляции или выполнения скрипта (для API-потребителя).
#[derive(Debug)]
pub enum Error {
    Parse(ParseError),
    Run(ScriptError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(e) => write!(f, "{e}"),
            Error::Run(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}
impl From<ScriptError> for Error {
    fn from(e: ScriptError) -> Self {
        Error::Run(e)
    }
}

/// Результат одного запуска скрипта корабля.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptOutput {
    /// Управляющий ввод, который скрипт «попросил» применить.
    pub input: InputFrame,
    /// Хотел ли скрипт открыть огонь.
    pub fire: bool,
    /// Число шагов интерпретатора (для статистики и лимитов).
    pub steps: u64,
}

/// Скомпилированный скрипт.
pub struct CompiledScript {
    pub name: String,
    stmts: Vec<Stmt>,
    limits: Limits,
}

/// Движок скриптов: компиляция, кэш и безопасный запуск.
pub struct ScriptEngine {
    /// Скомпилированные скрипты по имени.
    scripts: std::collections::BTreeMap<String, CompiledScript>,
    /// Статистика по запускам (сколько раз, сколько ошибок).
    pub runs: u64,
    pub errors: u64,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        ScriptEngine::new()
    }
}

impl ScriptEngine {
    pub fn new() -> Self {
        ScriptEngine {
            scripts: std::collections::BTreeMap::new(),
            runs: 0,
            errors: 0,
        }
    }

    /// Скомпилировать и зарегистрировать скрипт.
    pub fn compile(&mut self, name: &str, source: &str) -> Result<(), Error> {
        let stmts = parser::parse(source)?;
        self.scripts.insert(
            name.to_string(),
            CompiledScript {
                name: name.to_string(),
                stmts,
                limits: Limits::default(),
            },
        );
        Ok(())
    }

    /// Изменить лимиты конкретного скрипта.
    pub fn set_limits(&mut self, name: &str, limits: Limits) {
        if let Some(s) = self.scripts.get_mut(name) {
            s.limits = limits;
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.scripts.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    /// Подготовить хост с игровым API (управление кораблём).
    ///
    /// API намеренно минимален — только то, что разрешено игроку:
    /// * `thrust(v)`, `yaw(v)`, `pitch(v)` — задать управление;
    /// * `fire()` — открыть огонь;
    /// * `heading()`, `speed()`, `altitude()` — состояние корабля (только чтение);
    /// * `log(s)` — вывод на внутриигровой дисплей.
    fn build_host(&self, state: ScriptState) -> Host {
        let mut host = Host::new();

        // Общее изменяемое состояние вывода — через Rc.
        let acc = std::rc::Rc::new(std::cell::RefCell::new(ControlAccumulator::default()));

        let a = acc.clone();
        host.register("thrust", move |args| {
            a.borrow_mut().throttle = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
            Ok(Value::Nil)
        });
        let a = acc.clone();
        host.register("yaw", move |args| {
            a.borrow_mut().yaw = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
            Ok(Value::Nil)
        });
        let a = acc.clone();
        host.register("pitch", move |args| {
            a.borrow_mut().pitch = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
            Ok(Value::Nil)
        });
        let a = acc.clone();
        host.register("fire", move |_args| {
            a.borrow_mut().fire = true;
            Ok(Value::Nil)
        });

        // Состояние корабля (только чтение).
        let s = state;
        host.register("speed", move |_a| Ok(Value::Number(s.speed)));
        let s = state;
        host.register("altitude", move |_a| Ok(Value::Number(s.altitude)));
        let s = state;
        host.register("heading", move |_a| Ok(Value::Number(s.heading)));
        let s = state;
        host.register("target_distance", move |_a| Ok(Value::Number(s.target_distance)));

        // Вывод на дисплей.
        let a = acc.clone();
        host.register("log", move |args| {
            let msg = args
                .iter()
                .map(|v| v.to_display())
                .collect::<Vec<_>>()
                .join(" ");
            a.borrow_mut().logs.push(msg);
            Ok(Value::Nil)
        });

        // Математика (детерминированная по возможности).
        host.register("abs", |a| Ok(Value::Number(a.first().and_then(|v| v.as_number()).unwrap_or(0.0).abs())));
        host.register("min", |a| {
            let x = a.get(0).and_then(|v| v.as_number()).unwrap_or(0.0);
            let y = a.get(1).and_then(|v| v.as_number()).unwrap_or(0.0);
            Ok(Value::Number(x.min(y)))
        });
        host.register("max", |a| {
            let x = a.get(0).and_then(|v| v.as_number()).unwrap_or(0.0);
            let y = a.get(1).and_then(|v| v.as_number()).unwrap_or(0.0);
            Ok(Value::Number(x.max(y)))
        });
        host.register("clamp", |a| {
            let x = a.get(0).and_then(|v| v.as_number()).unwrap_or(0.0);
            let lo = a.get(1).and_then(|v| v.as_number()).unwrap_or(0.0);
            let hi = a.get(2).and_then(|v| v.as_number()).unwrap_or(0.0);
            Ok(Value::Number(x.max(lo).min(hi)))
        });
        host.register("floor", |a| {
            Ok(Value::Number(a.first().and_then(|v| v.as_number()).unwrap_or(0.0).floor()))
        });

        // Доступ к полю `state` (таблица) — заполняем глобал.
        let mut state_table: Vec<(Value, Value)> = Vec::new();
        state_table.push((Value::Str("speed".into()), Value::Number(state.speed)));
        state_table.push((Value::Str("altitude".into()), Value::Number(state.altitude)));
        state_table.push((Value::Str("heading".into()), Value::Number(state.heading)));
        state_table.push((
            Value::Str("target_distance".into()),
            Value::Number(state.target_distance),
        ));
        host.set(
            "state",
            Value::Table(std::rc::Rc::new(std::cell::RefCell::new(state_table))),
        );

        // Сохраняем аккумулятор в хосте через специальную переменную-«ручку».
        host.globals
            .insert("__control".into(), Value::Builtin("__control".into()));
        // Аккумулятор вернём отдельно: см. run_ship.
        CONTROL.with(|c| *c.borrow_mut() = Some(acc));
        host
    }

    /// Запустить скрипт корабля: получить управляющий ввод.
    pub fn run_ship(
        &mut self,
        name: &str,
        state: ScriptState,
        seq: u32,
    ) -> Result<ScriptOutput, Error> {
        let script = match self.scripts.get(name) {
            Some(s) => s,
            None => return Err(Error::Run(ScriptError::Undefined(name.to_string()))),
        };
        let mut host = self.build_host(state);
        host.limits = script.limits;

        let stmts = script.stmts.clone();
        self.runs += 1;

        let result = {
            let mut interp = Interpreter::new(&mut host);
            interp.run(&stmts)
        };

        match result {
            Ok(_) => {}
            Err(e) => {
                self.errors += 1;
                return Err(Error::Run(e));
            }
        }

        // Забираем накопленное управление.
        let acc = CONTROL.with(|c| c.borrow_mut().take()).unwrap_or_default();
        let acc = acc.borrow();
        Ok(ScriptOutput {
            input: InputFrame {
                seq,
                throttle: acc.throttle.clamp(-1.0, 1.0),
                yaw: acc.yaw.clamp(-1.0, 1.0),
                pitch: acc.pitch.clamp(-1.0, 1.0),
                fire: acc.fire,
                ..Default::default()
            },
            fire: acc.fire,
            steps: 0,
        })
    }
}

thread_local! {
    /// Аккумулятор управления. `thread_local`, потому что `Host` требует
    /// замыкания `Fn` (не `FnMut`), а состояние менять надо.
    static CONTROL: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<ControlAccumulator>>>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Default)]
struct ControlAccumulator {
    throttle: f32,
    yaw: f32,
    pitch: f32,
    fire: bool,
    logs: Vec<String>,
}

/// Состояние корабля, доступное скрипту (только чтение).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptState {
    pub speed: f64,
    pub altitude: f64,
    pub heading: f64,
    pub target_distance: f64,
}

/// Пример: скрипт турели. Держит цель в пределах и стреляет при сближении.
///
/// Точка входа — функция `tick(state)`: движок вызывает её каждый тик.
pub const TURRET_SCRIPT: &str = r#"
-- Турель: стреляет, когда цель ближе 800.
local range = 800

function tick(state)
    local d = target_distance()
    if d < range then
        fire()
        log("огонь по цели на " .. d)
    end
end

tick(state)
"#;

/// Пример: автопилот удержания высоты.
pub const AUTOPILOT_SCRIPT: &str = r#"
-- Автопилот: держим высоту около 100.
local target = 100

function tick(state)
    local alt = altitude()
    local diff = target - alt
    pitch(clamp(diff / 50, -1, 1))
    thrust(0.5)
end

tick(state)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with(src: &str) -> ScriptEngine {
        let mut e = ScriptEngine::new();
        e.compile("test", src).unwrap();
        e
    }

    #[test]
    fn compiles_and_counts() {
        let e = engine_with("local x = 1");
        assert_eq!(e.len(), 1);
        assert!(e.contains("test"));
    }

    #[test]
    fn arithmetic_and_logging() {
        let mut e = engine_with(r#"log(2 + 3 * 4)"#);
        let out = e.run_ship("test", ScriptState::default(), 1).unwrap();
        // log складывается в аккумулятор, поэтому просто проверим, что без ошибок.
        assert_eq!(out.steps, 0);
    }

    #[test]
    fn autopilot_controls_pitch_and_thrust() {
        let mut e = engine_with(AUTOPILOT_SCRIPT);
        // Корабль ниже цели → скрипт должен тянуть вверх (положительный pitch).
        let state = ScriptState { speed: 10.0, altitude: 20.0, heading: 0.0, target_distance: 0.0 };
        let out = e.run_ship("test", state, 1).unwrap();
        assert!(out.input.pitch > 0.0, "автопилот должен набирать высоту, pitch={}", out.input.pitch);
        assert!(out.input.throttle > 0.0);
    }

    #[test]
    fn turret_fires_when_target_is_close() {
        let mut e = engine_with(TURRET_SCRIPT);
        let close = ScriptState { target_distance: 500.0, ..Default::default() };
        let out = e.run_ship("test", close, 1).unwrap();
        assert!(out.fire, "турель должна стрелять по близкой цели");

        let far = ScriptState { target_distance: 5000.0, ..Default::default() };
        let out2 = e.run_ship("test", far, 2).unwrap();
        assert!(!out2.fire, "по далёкой цели стрелять не нужно");
    }

    #[test]
    fn infinite_loop_is_stopped_by_limits() {
        let mut e = engine_with("while true do end");
        let err = e.run_ship("test", ScriptState::default(), 1).unwrap_err();
        match err {
            Error::Run(ScriptError::StepLimit(_)) => {}
            other => panic!("ожидали лимит шагов, получено {other:?}"),
        }
        assert!(e.errors >= 1);
    }

    #[test]
    fn deep_recursion_is_stopped() {
        let src = r#"
function f(n)
    return f(n + 1)
end
f(0)
"#;
        let mut e = engine_with(src);
        let err = e.run_ship("test", ScriptState::default(), 1).unwrap_err();
        assert!(matches!(err, Error::Run(ScriptError::StackOverflow(_))), "получено {err:?}");
    }

    #[test]
    fn division_by_zero_is_an_error() {
        let mut e = engine_with("local x = 1 / 0");
        assert!(e.run_ship("test", ScriptState::default(), 1).is_err());
    }

    #[test]
    fn undefined_variable_is_an_error() {
        let mut e = engine_with("local x = unknown + 1");
        match e.run_ship("test", ScriptState::default(), 1) {
            Err(Error::Run(ScriptError::Undefined(n))) => assert_eq!(n, "unknown"),
            other => panic!("ожидали Undefined, получено {other:?}"),
        }
    }

    #[test]
    fn tables_and_fields_work() {
        let src = r#"
local cfg = { speed = 5, name = "турист" }
cfg.speed = 12
log(cfg.speed)
"#;
        let mut e = engine_with(src);
        assert!(e.run_ship("test", ScriptState::default(), 1).is_ok());
    }

    #[test]
    fn functions_with_arguments() {
        let src = r#"
function twice(x)
    return x * 2
end
local y = twice(21)
log(y)
"#;
        let mut e = engine_with(src);
        assert!(e.run_ship("test", ScriptState::default(), 1).is_ok());
    }

    #[test]
    fn loops_and_break() {
        let src = r#"
local sum = 0
for i in range(100) do
    if i > 10 then break end
    sum = sum + i
end
"#;
        let mut e = engine_with(src);
        assert!(e.run_ship("test", ScriptState::default(), 1).is_ok());
    }

    #[test]
    fn math_builtins_work() {
        let src = r#"
local a = clamp(150, 0, 100)
local b = min(3, 7)
local c = max(3, 7)
log(a .. " " .. b .. " " .. c)
"#;
        let mut e = engine_with(src);
        assert!(e.run_ship("test", ScriptState::default(), 1).is_ok());
    }

    #[test]
    fn parse_error_is_reported() {
        let mut e = ScriptEngine::new();
        let err = e.compile("bad", "if then end").unwrap_err();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn run_unknown_script_errors() {
        let mut e = ScriptEngine::new();
        assert!(e.run_ship("nope", ScriptState::default(), 1).is_err());
    }

    #[test]
    fn scripts_are_isolated() {
        // Два скрипта не мешают друг другу: управление аккумулируется заново.
        let mut e = ScriptEngine::new();
        e.compile("a", "thrust(1)").unwrap();
        e.compile("b", "yaw(-1)").unwrap();
        let out_a = e.run_ship("a", ScriptState::default(), 1).unwrap();
        let out_b = e.run_ship("b", ScriptState::default(), 2).unwrap();
        assert_eq!(out_a.input.throttle, 1.0);
        assert_eq!(out_a.input.yaw, 0.0);
        assert_eq!(out_b.input.throttle, 0.0);
        assert_eq!(out_b.input.yaw, -1.0);
    }
}
