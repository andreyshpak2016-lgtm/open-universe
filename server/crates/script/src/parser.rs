//! Парсер OpenScript: токены → AST.
//!
//! Грамматика — подмножество Lua: локальные переменные, присваивание,
//! арифметика и логика, `if/elseif/else`, `while`, `for ... in`, функции,
//! вызовы, таблицы (`{}`) и доступ к полям (`t.x`, `t["x"]`).

use crate::lexer::{lex, ParseError, Pos, Token};

/// Выражение.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    Nil,
    /// Имя переменной.
    Var(String),
    /// `a op b`.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// `-a`, `not a`.
    Un(UnOp, Box<Expr>),
    /// Вызов функции: `f(args)`.
    Call(String, Vec<Expr>),
    /// Доступ к полю: `obj.name`.
    Field(Box<Expr>, String),
    /// Индексация: `obj[expr]`.
    Index(Box<Expr>, Box<Expr>),
    /// Таблица: `{ k = v, [e] = v, v }`.
    Table(Vec<(Option<Expr>, Expr)>),
    /// Логическое `and`/`or` без вычисления лишнего операнда реализуем через Bin.
    Function(Vec<String>, Vec<Stmt>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Concat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

/// Оператор (statement).
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `local a, b = e1, e2`.
    Local(Vec<String>, Vec<Expr>),
    /// `a = e` или `a.b = e` / `a[e] = e`.
    Assign(Expr, Expr),
    /// Выражение-оператор (вызов).
    Expr(Expr),
    /// `if c then ... elseif ... else ... end`.
    If(Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    /// `while c do ... end`.
    While(Expr, Vec<Stmt>),
    /// `for k, v in pairs(t) do ... end`.
    ForIn(Vec<String>, Expr, Vec<Stmt>),
    /// `return e`.
    Return(Option<Expr>),
    /// `break`.
    Break,
    /// `function name(params) ... end`.
    FunctionDecl(String, Vec<String>, Vec<Stmt>),
}

struct Parser {
    toks: Vec<(Token, Pos)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.pos].0
    }
    fn peek_pos(&self) -> Pos {
        self.toks[self.pos].1
    }
    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].0.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn check(&self, t: &Token) -> bool {
        self.peek() == t
    }
    fn accept(&mut self, t: &Token) -> bool {
        if self.check(t) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Token) -> Result<(), ParseError> {
        if self.accept(t) {
            Ok(())
        } else {
            Err(ParseError {
                pos: self.peek_pos(),
                message: format!("ожидалось {}, найдено {}", t.describe(), self.peek().describe()),
            })
        }
    }

    /// Разбор блока до стоп-слова.
    fn block(&mut self, stops: &[Token]) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            if stops.iter().any(|s| self.check(s)) || self.check(&Token::Eof) {
                break;
            }
            stmts.push(self.statement()?);
        }
        Ok(stmts)
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().clone() {
            Token::Semicolon => {
                self.advance();
                self.statement()
            }
            Token::Local => {
                self.advance();
                let mut names = vec![self.expect_name()?];
                while self.accept(&Token::Comma) {
                    names.push(self.expect_name()?);
                }
                let mut exprs = Vec::new();
                if self.accept(&Token::Eq) {
                    exprs.push(self.expr()?);
                    while self.accept(&Token::Comma) {
                        exprs.push(self.expr()?);
                    }
                }
                Ok(Stmt::Local(names, exprs))
            }
            Token::Function => {
                // Два вида: `function name(...) ... end` или локальная функция.
                self.advance();
                let name = self.expect_name()?;
                self.expect(&Token::LParen)?;
                let params = self.name_list()?;
                self.expect(&Token::RParen)?;
                let body = self.block(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Stmt::FunctionDecl(name, params, body))
            }
            Token::If => {
                self.advance();
                let cond = self.expr()?;
                self.expect(&Token::Then)?;
                let then = self.block(&[Token::ElseIf, Token::Else, Token::End])?;
                let mut branches = vec![(cond, then)];
                while self.accept(&Token::ElseIf) {
                    let c = self.expr()?;
                    self.expect(&Token::Then)?;
                    let b = self.block(&[Token::ElseIf, Token::Else, Token::End])?;
                    branches.push((c, b));
                }
                let els = if self.accept(&Token::Else) {
                    Some(self.block(&[Token::End])?)
                } else {
                    None
                };
                self.expect(&Token::End)?;
                Ok(Stmt::If(branches, els))
            }
            Token::While => {
                self.advance();
                let cond = self.expr()?;
                self.expect(&Token::Do)?;
                let body = self.block(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Stmt::While(cond, body))
            }
            Token::For => {
                self.advance();
                let mut names = vec![self.expect_name()?];
                while self.accept(&Token::Comma) {
                    names.push(self.expect_name()?);
                }
                self.expect(&Token::In)?;
                let iter = self.expr()?;
                self.expect(&Token::Do)?;
                let body = self.block(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Stmt::ForIn(names, iter, body))
            }
            Token::Return => {
                self.advance();
                let e = if matches!(self.peek(), Token::End | Token::Eof | Token::Semicolon) {
                    None
                } else {
                    Some(self.expr()?)
                };
                Ok(Stmt::Return(e))
            }
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            _ => {
                // Присваивание или выражение-оператор.
                let e = self.suffixed()?;
                if self.accept(&Token::Eq) {
                    let value = self.expr()?;
                    Ok(Stmt::Assign(e, value))
                } else {
                    Ok(Stmt::Expr(e))
                }
            }
        }
    }

    fn expect_name(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Name(n) => Ok(n),
            other => Err(ParseError {
                pos: self.peek_pos(),
                message: format!("ожидалось имя, найдено {}", other.describe()),
            }),
        }
    }

    fn name_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut names = Vec::new();
        if self.check(&Token::RParen) {
            return Ok(names);
        }
        names.push(self.expect_name()?);
        while self.accept(&Token::Comma) {
            names.push(self.expect_name()?);
        }
        Ok(names)
    }

    // --- Выражения ---

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and_expr()?;
        while self.accept(&Token::Or) {
            let right = self.and_expr()?;
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.cmp_expr()?;
        while self.accept(&Token::And) {
            let right = self.cmp_expr()?;
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn cmp_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.concat_expr()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                Token::Lt => BinOp::Lt,
                Token::Le => BinOp::Le,
                Token::Gt => BinOp::Gt,
                Token::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.concat_expr()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn concat_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.add_expr()?;
        while self.accept(&Token::DotDot) {
            let right = self.add_expr()?;
            left = Expr::Bin(BinOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn add_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.mul_expr()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn mul_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary_expr()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.unary_expr()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary_expr(&mut self) -> Result<Expr, ParseError> {
        if self.accept(&Token::Minus) {
            return Ok(Expr::Un(UnOp::Neg, Box::new(self.unary_expr()?)));
        }
        if self.accept(&Token::Not) {
            return Ok(Expr::Un(UnOp::Not, Box::new(self.unary_expr()?)));
        }
        self.pow_expr()
    }

    fn pow_expr(&mut self) -> Result<Expr, ParseError> {
        let base = self.suffixed()?;
        if self.accept(&Token::Caret) {
            // Правоассоциативно.
            let exp = self.unary_expr()?;
            return Ok(Expr::Bin(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    /// Первичное выражение с постфиксами (`.x`, `[e]`, `(args)`).
    fn suffixed(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.primary()?;
        loop {
            if self.accept(&Token::Dot) {
                let name = self.expect_name()?;
                e = Expr::Field(Box::new(e), name);
            } else if self.accept(&Token::LBracket) {
                let idx = self.expr()?;
                self.expect(&Token::RBracket)?;
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else if self.check(&Token::LParen) {
                // Вызов: только для имени или поля (упрощение).
                self.advance();
                let mut args = Vec::new();
                if !self.check(&Token::RParen) {
                    args.push(self.expr()?);
                    while self.accept(&Token::Comma) {
                        args.push(self.expr()?);
                    }
                }
                self.expect(&Token::RParen)?;
                let name = match e {
                    Expr::Var(n) => n,
                    Expr::Field(_, n) => n,
                    other => {
                        return Err(ParseError {
                            pos: self.peek_pos(),
                            message: format!("нельзя вызвать выражение {other:?}"),
                        })
                    }
                };
                e = Expr::Call(name, args);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.advance() {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Nil => Ok(Expr::Nil),
            Token::Name(n) => Ok(Expr::Var(n)),
            Token::LParen => {
                let e = self.expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Token::LBrace => {
                let mut fields = Vec::new();
                if !self.check(&Token::RBrace) {
                    loop {
                        // Формы: `name = v`, `[e] = v`, `v`.
                        if let Token::Name(n) = self.peek().clone() {
                            if self.toks[self.pos + 1].0 == Token::Eq {
                                self.advance();
                                self.advance();
                                let v = self.expr()?;
                                fields.push((Some(Expr::Str(n)), v));
                            } else {
                                let v = self.expr()?;
                                fields.push((None, v));
                            }
                        } else if self.accept(&Token::LBracket) {
                            let k = self.expr()?;
                            self.expect(&Token::RBracket)?;
                            self.expect(&Token::Eq)?;
                            let v = self.expr()?;
                            fields.push((Some(k), v));
                        } else {
                            let v = self.expr()?;
                            fields.push((None, v));
                        }
                        if !self.accept(&Token::Comma) && !self.accept(&Token::Semicolon) {
                            break;
                        }
                        if self.check(&Token::RBrace) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::Table(fields))
            }
            Token::Function => {
                self.expect(&Token::LParen)?;
                let params = self.name_list()?;
                self.expect(&Token::RParen)?;
                let body = self.block(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Expr::Function(params, body))
            }
            other => Err(ParseError {
                pos: self.peek_pos(),
                message: format!("неожиданный токен {}", other.describe()),
            }),
        }
    }
}

/// Разобрать исходник в список операторов.
pub fn parse(src: &str) -> Result<Vec<Stmt>, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let stmts = p.block(&[])?;
    p.expect(&Token::Eof)?;
    Ok(stmts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_assignment() {
        let s = parse("local x = 1 + 2 * 3").unwrap();
        match &s[0] {
            Stmt::Local(names, exprs) => {
                assert_eq!(names, &vec!["x".to_string()]);
                assert_eq!(exprs.len(), 1);
            }
            other => panic!("не то: {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let s = parse("local x = 1 + 2 * 3").unwrap();
        if let Stmt::Local(_, exprs) = &s[0] {
            // Ожидаем Add(1, Mul(2,3)).
            match &exprs[0] {
                Expr::Bin(BinOp::Add, _, r) => assert!(matches!(**r, Expr::Bin(BinOp::Mul, _, _))),
                other => panic!("неверный разбор: {other:?}"),
            }
        }
    }

    #[test]
    fn parses_if_elseif_else() {
        let src = "if a then x = 1 elseif b then x = 2 else x = 3 end";
        let s = parse(src).unwrap();
        match &s[0] {
            Stmt::If(branches, els) => {
                assert_eq!(branches.len(), 2);
                assert!(els.is_some());
            }
            other => panic!("не то: {other:?}"),
        }
    }

    #[test]
    fn parses_function_decl_and_call() {
        let src = "function add(a, b) return a + b end\nlocal c = add(1, 2)";
        let s = parse(src).unwrap();
        assert!(matches!(s[0], Stmt::FunctionDecl(_, _, _)));
        if let Stmt::Local(_, e) = &s[1] {
            assert!(matches!(e[0], Expr::Call(_, _)));
        }
    }

    #[test]
    fn parses_tables_and_fields() {
        let src = r#"local t = { speed = 10, ["x"] = 1 }
local v = t.speed"#;
        let s = parse(src).unwrap();
        if let Stmt::Local(_, e) = &s[0] {
            assert!(matches!(e[0], Expr::Table(_)));
        }
        if let Stmt::Local(_, e) = &s[1] {
            assert!(matches!(e[0], Expr::Field(_, _)));
        }
    }

    #[test]
    fn parses_while_and_break() {
        let s = parse("while true do break end").unwrap();
        match &s[0] {
            Stmt::While(_, body) => assert!(matches!(body[0], Stmt::Break)),
            other => panic!("не то: {other:?}"),
        }
    }

    #[test]
    fn parses_for_in() {
        let s = parse("for k, v in pairs(t) do end").unwrap();
        match &s[0] {
            Stmt::ForIn(names, _, _) => assert_eq!(names.len(), 2),
            other => panic!("не то: {other:?}"),
        }
    }

    #[test]
    fn errors_on_missing_end() {
        assert!(parse("if a then").is_err());
    }

    #[test]
    fn errors_on_bad_token() {
        assert!(parse("local = 5").is_err());
    }
}
