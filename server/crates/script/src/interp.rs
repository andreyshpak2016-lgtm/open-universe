//! Интерпретатор OpenScript с песочницей.
//!
//! ## Зачем песочница
//! Скрипты пишут игроки, а сервер единый. Один бесконечный цикл может
//! остановить мир для всех. Поэтому у интерпретатора жёсткие лимиты:
//! * число шагов выполнения (защита от `while true`);
//! * глубина рекурсии (защита от переполнения стека);
//! * размер таблиц (защита от «пожирания» памяти).
//!
//! Скрипт **не имеет** доступа к файлам, сети и памяти — только к явно
//! переданному API (`host`-функции): `thrust`, `scan`, `fire`, `log` и т. п.

use crate::parser::{BinOp, Expr, Stmt, UnOp};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;

/// Значение в скрипте.
#[derive(Clone, Debug)]
pub enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
    Nil,
    /// Таблица. Разделяемая (`Rc<RefCell>`), чтобы мутировалась по месту —
    /// как в Lua: `t.x = 1` меняет таблицу, а не копию.
    Table(Rc<RefCell<Vec<(Value, Value)>>>),
    /// Встроенная функция хоста.
    Builtin(String),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Value {
    /// Правдиво ли значение (в Lua ложь — только `false` и `nil`).
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Bool(false) | Value::Nil)
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Новая пустая таблица.
    pub fn new_table() -> Value {
        Value::Table(Rc::new(RefCell::new(Vec::new())))
    }

    /// Текстовое представление (как `tostring` в Lua).
    pub fn to_display(&self) -> String {
        match self {
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Nil => "nil".into(),
            Value::Table(_) => "table".into(),
            Value::Builtin(name) => format!("<builtin {name}>"),
        }
    }

    /// Преобразование в строку (для `tostring` и конкатенации).
    pub fn as_str(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}

/// Ошибка выполнения скрипта.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptError {
    StepLimit(u64),
    StackOverflow(usize),
    Runtime(String),
    Undefined(String),
    Parse(String),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::StepLimit(n) => {
                write!(f, "превышен лимит шагов ({n}) — возможно, бесконечный цикл")
            }
            ScriptError::StackOverflow(n) => write!(f, "слишком глубокая рекурсия ({n})"),
            ScriptError::Runtime(m) => write!(f, "ошибка выполнения: {m}"),
            ScriptError::Undefined(name) => write!(f, "неизвестная переменная '{name}'"),
            ScriptError::Parse(m) => write!(f, "ошибка разбора: {m}"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Лимиты песочницы.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_steps: u64,
    pub max_depth: usize,
    pub max_table_size: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_steps: 200_000, max_depth: 64, max_table_size: 1024 }
    }
}

/// Встроенная функция, доступная скрипту.
pub type HostFn = Rc<dyn Fn(&[Value]) -> Result<Value, ScriptError>>;

/// Хост: переменные, встроенные функции, лимиты и вывод.
pub struct Host {
    pub globals: HashMap<String, Value>,
    pub builtins: HashMap<String, HostFn>,
    pub limits: Limits,
    /// Вывод скрипта (`log(...)`) — для отладки и внутриигровых дисплеев.
    pub output: Vec<String>,
}

impl Default for Host {
    fn default() -> Self {
        Host::new()
    }
}

impl Host {
    pub fn new() -> Self {
        Host {
            globals: HashMap::new(),
            builtins: HashMap::new(),
            limits: Limits::default(),
            output: Vec::new(),
        }
    }

    /// Зарегистрировать встроенную функцию.
    pub fn register<F>(&mut self, name: &str, f: F)
    where
        F: Fn(&[Value]) -> Result<Value, ScriptError> + 'static,
    {
        self.builtins.insert(name.to_string(), Rc::new(f));
    }

    /// Задать глобальную переменную.
    pub fn set(&mut self, name: &str, v: Value) {
        self.globals.insert(name.to_string(), v);
    }
}

struct Scope {
    vars: HashMap<String, Value>,
}

enum Flow {
    Normal,
    Return(Value),
    Break,
}

/// Интерпретатор.
pub struct Interpreter<'a> {
    host: &'a mut Host,
    scopes: Vec<Scope>,
    /// Пользовательские функции (имя → параметры и тело).
    functions: HashMap<String, (Vec<String>, Vec<Stmt>)>,
    steps: u64,
    depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(host: &'a mut Host) -> Self {
        let mut scopes = vec![Scope {
            vars: host.globals.clone(),
        }];
        let _ = &mut scopes;
        Interpreter {
            host,
            scopes,
            functions: HashMap::new(),
            steps: 0,
            depth: 0,
        }
    }

    fn tick(&mut self) -> Result<(), ScriptError> {
        self.steps += 1;
        if self.steps > self.host.limits.max_steps {
            return Err(ScriptError::StepLimit(self.host.limits.max_steps));
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.vars.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn declare(&mut self, name: &str, v: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.to_string(), v);
        }
    }

    fn assign(&mut self, name: &str, v: Value) {
        // Если переменная есть в каком-то кадре — обновляем там.
        for scope in self.scopes.iter_mut().rev() {
            if scope.vars.contains_key(name) {
                scope.vars.insert(name.to_string(), v);
                return;
            }
        }
        // Иначе создаём в текущем кадре.
        self.declare(name, v);
    }

    /// Выполнить список операторов.
    pub fn run(&mut self, stmts: &[Stmt]) -> Result<Value, ScriptError> {
        match self.exec_block(stmts)? {
            Flow::Return(v) => Ok(v),
            Flow::Break => Err(ScriptError::Runtime("break вне цикла".into())),
            Flow::Normal => Ok(Value::Nil),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow, ScriptError> {
        for s in stmts {
            match self.exec_stmt(s)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, s: &Stmt) -> Result<Flow, ScriptError> {
        self.tick()?;
        match s {
            Stmt::Local(names, exprs) => {
                let mut vals = Vec::with_capacity(exprs.len());
                for e in exprs {
                    vals.push(self.eval(e)?);
                }
                for (i, name) in names.iter().enumerate() {
                    let v = vals.get(i).cloned().unwrap_or(Value::Nil);
                    self.declare(name, v);
                }
                Ok(Flow::Normal)
            }
            Stmt::Assign(target, value) => {
                match target {
                    Expr::Var(name) => {
                        let v = self.eval(value)?;
                        self.assign(name, v);
                    }
                    Expr::Field(obj, field) => {
                        // Таблицу берём по ссылке и мутируем по месту.
                        let table = self.eval_table_ref(obj)?;
                        let v = self.eval(value)?;
                        set_field_in(&table, &Value::Str(field.clone()), v)?;
                    }
                    Expr::Index(obj, idx) => {
                        let table = self.eval_table_ref(obj)?;
                        let key = self.eval(idx)?;
                        let v = self.eval(value)?;
                        set_field_in(&table, &key, v)?;
                    }
                    other => {
                        return Err(ScriptError::Runtime(format!("нельзя присвоить в {other:?}")))
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Expr(e) => {
                self.eval(e)?;
                Ok(Flow::Normal)
            }
            Stmt::If(branches, els) => {
                for (cond, body) in branches {
                    if self.eval(cond)?.truthy() {
                        self.push_scope();
                        let r = self.exec_block(body);
                        self.pop_scope();
                        return r;
                    }
                }
                if let Some(body) = els {
                    self.push_scope();
                    let r = self.exec_block(body);
                    self.pop_scope();
                    return r;
                }
                Ok(Flow::Normal)
            }
            Stmt::While(cond, body) => {
                while self.eval(cond)?.truthy() {
                    self.tick()?;
                    self.push_scope();
                    let r = self.exec_block(body);
                    self.pop_scope();
                    match r? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal => {}
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::ForIn(names, iter, body) => {
                let items = self.iter_items(iter)?;
                for (k, v) in items {
                    self.tick()?;
                    self.push_scope();
                    self.declare(&names[0], k);
                    if names.len() > 1 {
                        self.declare(&names[1], v);
                    }
                    let r = self.exec_block(body);
                    self.pop_scope();
                    match r? {
                        Flow::Break => break,
                        Flow::Return(val) => return Ok(Flow::Return(val)),
                        Flow::Normal => {}
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(expr) => self.eval(expr)?,
                    None => Value::Nil,
                };
                Ok(Flow::Return(v))
            }
            Stmt::Break => Ok(Flow::Break),
            Stmt::FunctionDecl(name, params, body) => {
                self.functions.insert(name.clone(), (params.clone(), body.clone()));
                self.assign(name, Value::Builtin(name.clone()));
                Ok(Flow::Normal)
            }
        }
    }

    /// Получить ссылку на таблицу из выражения (для присваивания по месту).
    fn eval_table_ref(&mut self, e: &Expr) -> Result<Rc<RefCell<Vec<(Value, Value)>>>, ScriptError> {
        let v = self.eval(e)?;
        match v {
            Value::Table(t) => Ok(t),
            other => Err(ScriptError::Runtime(format!(
                "{} — не таблица",
                other.to_display()
            ))),
        }
    }

    /// Элементы для `for ... in`.
    fn iter_items(&mut self, e: &Expr) -> Result<Vec<(Value, Value)>, ScriptError> {
        match e {
            Expr::Call(fname, args) if fname == "pairs" => {
                let t = match args.first() {
                    Some(a) => self.eval(a)?,
                    None => Value::Nil,
                };
                match t {
                    Value::Table(rc) => Ok(rc.borrow().clone()),
                    other => Err(ScriptError::Runtime(format!(
                        "pairs ожидает таблицу, получено {}",
                        other.to_display()
                    ))),
                }
            }
            Expr::Call(fname, args) if fname == "range" => {
                let n = match args.first() {
                    Some(a) => self.eval(a)?,
                    None => Value::Nil,
                };
                let n = n
                    .as_number()
                    .ok_or_else(|| ScriptError::Runtime("range ожидает число".into()))?
                    as i64;
                // Ограничиваем диапазон, чтобы не обойти песочницу.
                if n > 100_000 {
                    return Err(ScriptError::Runtime("range слишком большой".into()));
                }
                Ok((1..=n)
                    .map(|i| (Value::Number(i as f64), Value::Number(i as f64)))
                    .collect())
            }
            other => {
                let t = self.eval(other)?;
                match t {
                    Value::Table(rc) => Ok(rc.borrow().clone()),
                    _ => Err(ScriptError::Runtime(
                        "нельзя итерировать это значение".into(),
                    )),
                }
            }
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope { vars: HashMap::new() });
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, ScriptError> {
        self.tick()?;
        match e {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Var(name) => self
                .lookup(name)
                .ok_or_else(|| ScriptError::Undefined(name.clone())),
            Expr::Un(op, inner) => {
                let v = self.eval(inner)?;
                match op {
                    UnOp::Neg => v
                        .as_number()
                        .map(|n| Value::Number(-n))
                        .ok_or_else(|| ScriptError::Runtime("минус применим только к числу".into())),
                    UnOp::Not => Ok(Value::Bool(!v.truthy())),
                }
            }
            Expr::Bin(op, l, r) => self.eval_bin(*op, l, r),
            Expr::Field(obj, field) => {
                let base = self.eval(obj)?;
                get_field(&base, &Value::Str(field.clone()))
            }
            Expr::Index(obj, idx) => {
                let base = self.eval(obj)?;
                let key = self.eval(idx)?;
                get_field(&base, &key)
            }
            Expr::Table(fields) => {
                if fields.len() > self.host.limits.max_table_size {
                    return Err(ScriptError::Runtime("таблица слишком большая".into()));
                }
                let table = Value::new_table();
                let rc = match &table {
                    Value::Table(t) => t.clone(),
                    _ => unreachable!(),
                };
                for (i, (k, v)) in fields.iter().enumerate() {
                    let val = self.eval(v)?;
                    let key = match k {
                        Some(ke) => self.eval(ke)?,
                        None => Value::Number((i + 1) as f64),
                    };
                    rc.borrow_mut().push((key, val));
                }
                Ok(table)
            }
            Expr::Function(params, body) => {
                // Анонимная функция: регистрируем под уникальным именем.
                let name = format!("<anon{}>", self.functions.len());
                self.functions.insert(name.clone(), (params.clone(), body.clone()));
                Ok(Value::Builtin(name))
            }
            Expr::Call(name, args) => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a)?);
                }
                // 1. Пользовательская функция.
                if self.functions.contains_key(name) {
                    return self.call_user(name, &vals);
                }
                // 2. Встроенная функция хоста.
                let b = self.host.builtins.get(name).cloned();
                if let Some(b) = b {
                    return b(&vals);
                }
                // 3. Значение-функция в переменной.
                if let Some(Value::Builtin(fname)) = self.lookup(name) {
                    if self.functions.contains_key(&fname) {
                        return self.call_user(&fname, &vals);
                    }
                    if let Some(b) = self.host.builtins.get(&fname).cloned() {
                        return b(&vals);
                    }
                }
                Err(ScriptError::Undefined(name.clone()))
            }
        }
    }

    fn eval_bin(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<Value, ScriptError> {
        // Короткое замыкание.
        if op == BinOp::And {
            let lv = self.eval(l)?;
            return if lv.truthy() { self.eval(r) } else { Ok(lv) };
        }
        if op == BinOp::Or {
            let lv = self.eval(l)?;
            return if lv.truthy() { Ok(lv) } else { self.eval(r) };
        }

        let a = self.eval(l)?;
        let b = self.eval(r)?;

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow => {
                let x = a
                    .as_number()
                    .ok_or_else(|| ScriptError::Runtime(format!("{} не число", a.to_display())))?;
                let y = b
                    .as_number()
                    .ok_or_else(|| ScriptError::Runtime(format!("{} не число", b.to_display())))?;
                let v = match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => {
                        if y == 0.0 {
                            return Err(ScriptError::Runtime("деление на ноль".into()));
                        }
                        x / y
                    }
                    BinOp::Mod => {
                        if y == 0.0 {
                            return Err(ScriptError::Runtime("деление на ноль".into()));
                        }
                        x % y
                    }
                    BinOp::Pow => x.powf(y),
                    _ => unreachable!(),
                };
                Ok(Value::Number(v))
            }
            BinOp::Concat => Ok(Value::Str(format!("{}{}", a.to_display(), b.to_display()))),
            BinOp::Eq => Ok(Value::Bool(a == b)),
            BinOp::NotEq => Ok(Value::Bool(a != b)),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let x = a
                    .as_number()
                    .ok_or_else(|| ScriptError::Runtime("сравнение только для чисел".into()))?;
                let y = b
                    .as_number()
                    .ok_or_else(|| ScriptError::Runtime("сравнение только для чисел".into()))?;
                let res = match op {
                    BinOp::Lt => x < y,
                    BinOp::Le => x <= y,
                    BinOp::Gt => x > y,
                    BinOp::Ge => x >= y,
                    _ => unreachable!(),
                };
                Ok(Value::Bool(res))
            }
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn call_user(&mut self, name: &str, args: &[Value]) -> Result<Value, ScriptError> {
        self.depth += 1;
        if self.depth > self.host.limits.max_depth {
            self.depth -= 1;
            return Err(ScriptError::StackOverflow(self.host.limits.max_depth));
        }
        let (params, body) = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| ScriptError::Undefined(name.to_string()))?;

        self.push_scope();
        for (i, p) in params.iter().enumerate() {
            let v = args.get(i).cloned().unwrap_or(Value::Nil);
            self.declare(p, v);
        }
        let r = self.exec_block(&body);
        self.pop_scope();
        self.depth -= 1;
        match r? {
            Flow::Return(v) => Ok(v),
            Flow::Normal => Ok(Value::Nil),
            Flow::Break => Err(ScriptError::Runtime("break вне цикла".into())),
        }
    }
}

/// Чтение поля таблицы.
fn get_field(base: &Value, key: &Value) -> Result<Value, ScriptError> {
    match base {
        Value::Table(rc) => Ok(rc
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Nil)),
        Value::Nil => Err(ScriptError::Runtime("обращение к полю nil".into())),
        other => Err(ScriptError::Runtime(format!(
            "у {} нет полей",
            other.to_display()
        ))),
    }
}

/// Запись поля таблицы (по месту).
fn set_field_in(
    table: &Rc<RefCell<Vec<(Value, Value)>>>,
    key: &Value,
    value: Value,
) -> Result<(), ScriptError> {
    let mut t = table.borrow_mut();
    if let Some(slot) = t.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value;
    } else {
        t.push((key.clone(), value));
    }
    Ok(())
}
