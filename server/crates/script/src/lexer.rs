//! Лексер языка OpenScript (Lua-подобный).
//!
//! Разбирает исходник на токены: числа, строки, имена, ключевые слова и
//! операторы. Все номера строк сохраняются — они нужны для понятных ошибок
//! игрокам и для защиты (лимит на глубину/время выполнения).

use std::fmt;

/// Позиция в исходнике (для сообщений об ошибках).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Токен языка.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Литералы
    Number(f64),
    Str(String),
    Name(String),
    // Ключевые слова
    Local,
    Function,
    End,
    If,
    Then,
    ElseIf,
    Else,
    While,
    Do,
    For,
    In,
    Return,
    Break,
    True,
    False,
    Nil,
    And,
    Or,
    Not,
    // Операторы и пунктуация
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Eq,      // =
    EqEq,    // ==
    NotEq,   // ~=
    Lt,      // <
    Le,      // <=
    Gt,      // >
    Ge,      // >=
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Dot,
    DotDot,  // ..
    // Служебные
    Eof,
}

impl Token {
    /// Человекочитаемое имя (для ошибок).
    pub fn describe(&self) -> String {
        match self {
            Token::Number(n) => format!("число {n}"),
            Token::Str(_) => "строка".into(),
            Token::Name(n) => format!("имя '{n}'"),
            Token::Eof => "конец файла".into(),
            other => format!("'{other:?}'"),
        }
    }
}

/// Ошибка лексики/парсинга.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub pos: Pos,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Разбить исходник на токены.
pub fn lex(src: &str) -> Result<Vec<(Token, Pos)>, ParseError> {
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;
    let mut out = Vec::new();

    // Продвинуть курсор на один символ.
    macro_rules! bump {
        () => {{
            if bytes[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < bytes.len() {
        let c = bytes[i];
        let pos = Pos { line, col };

        // Пробелы.
        if c.is_whitespace() {
            bump!();
            continue;
        }

        // Комментарии: -- до конца строки, --[[ ... ]] блочный.
        if c == '-' && i + 1 < bytes.len() && bytes[i + 1] == '-' {
            bump!();
            bump!();
            if i + 1 < bytes.len() && bytes[i] == '[' && bytes[i + 1] == '[' {
                bump!();
                bump!();
                while i + 1 < bytes.len() && !(bytes[i] == ']' && bytes[i + 1] == ']') {
                    bump!();
                }
                if i + 1 < bytes.len() {
                    bump!();
                    bump!();
                }
            } else {
                while i < bytes.len() && bytes[i] != '\n' {
                    bump!();
                }
            }
            continue;
        }

        // Числа (целые и дробные).
        if c.is_ascii_digit() || (c == '.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()) {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                bump!();
            }
            // Экспонента.
            if i < bytes.len() && (bytes[i] == 'e' || bytes[i] == 'E') {
                bump!();
                if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
                    bump!();
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    bump!();
                }
            }
            let text: String = bytes[start..i].iter().collect();
            let n: f64 = text.parse().map_err(|_| ParseError {
                pos,
                message: format!("некорректное число '{text}'"),
            })?;
            out.push((Token::Number(n), pos));
            continue;
        }

        // Строки: '...' и "..." с экранированием.
        if c == '"' || c == '\'' {
            let quote = c;
            bump!();
            let mut s = String::new();
            loop {
                if i >= bytes.len() {
                    return Err(ParseError { pos, message: "незакрытая строка".into() });
                }
                let ch = bytes[i];
                if ch == quote {
                    bump!();
                    break;
                }
                if ch == '\\' {
                    bump!();
                    if i >= bytes.len() {
                        return Err(ParseError { pos, message: "незакрытое экранирование".into() });
                    }
                    let esc = bytes[i];
                    s.push(match esc {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        '"' => '"',
                        '\'' => '\'',
                        other => other,
                    });
                    bump!();
                    continue;
                }
                s.push(ch);
                bump!();
            }
            out.push((Token::Str(s), pos));
            continue;
        }

        // Имена и ключевые слова.
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_alphanumeric() || bytes[i] == '_') {
                bump!();
            }
            let word: String = bytes[start..i].iter().collect();
            let tok = match word.as_str() {
                "local" => Token::Local,
                "function" => Token::Function,
                "end" => Token::End,
                "if" => Token::If,
                "then" => Token::Then,
                "elseif" => Token::ElseIf,
                "else" => Token::Else,
                "while" => Token::While,
                "do" => Token::Do,
                "for" => Token::For,
                "in" => Token::In,
                "return" => Token::Return,
                "break" => Token::Break,
                "true" => Token::True,
                "false" => Token::False,
                "nil" => Token::Nil,
                "and" => Token::And,
                "or" => Token::Or,
                "not" => Token::Not,
                _ => Token::Name(word),
            };
            out.push((tok, pos));
            continue;
        }

        // Операторы и пунктуация.
        let two = |a: char, b: char| -> bool { i + 1 < bytes.len() && bytes[i] == a && bytes[i + 1] == b };
        let tok = if two('=', '=') {
            bump!(); bump!(); Token::EqEq
        } else if two('~', '=') {
            bump!(); bump!(); Token::NotEq
        } else if two('<', '=') {
            bump!(); bump!(); Token::Le
        } else if two('>', '=') {
            bump!(); bump!(); Token::Ge
        } else if two('.', '.') {
            bump!(); bump!(); Token::DotDot
        } else {
            let t = match c {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '%' => Token::Percent,
                '^' => Token::Caret,
                '=' => Token::Eq,
                '<' => Token::Lt,
                '>' => Token::Gt,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                '[' => Token::LBracket,
                ']' => Token::RBracket,
                ',' => Token::Comma,
                ';' => Token::Semicolon,
                '.' => Token::Dot,
                other => {
                    return Err(ParseError {
                        pos,
                        message: format!("неожиданный символ '{other}'"),
                    })
                }
            };
            bump!();
            t
        };
        out.push((tok, pos));
    }

    out.push((Token::Eof, Pos { line, col }));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Token> {
        lex(src).unwrap().into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn lexes_numbers() {
        assert_eq!(kinds("42 3.5 1e3"), vec![
            Token::Number(42.0),
            Token::Number(3.5),
            Token::Number(1000.0),
            Token::Eof
        ]);
    }

    #[test]
    fn lexes_strings_with_escapes() {
        let k = kinds(r#""hi\n" 'a\'b'"#);
        assert_eq!(k[0], Token::Str("hi\n".into()));
        assert_eq!(k[1], Token::Str("a'b".into()));
    }

    #[test]
    fn lexes_keywords_and_names() {
        let k = kinds("local x = true");
        assert_eq!(k, vec![
            Token::Local,
            Token::Name("x".into()),
            Token::Eq,
            Token::True,
            Token::Eof
        ]);
    }

    #[test]
    fn lexes_operators() {
        assert_eq!(kinds("== ~= <= >= .. ^"), vec![
            Token::EqEq, Token::NotEq, Token::Le, Token::Ge, Token::DotDot,
            Token::Caret, Token::Eof
        ]);
    }

    #[test]
    fn skips_comments() {
        let k = kinds("-- строка\nx --[[ блок ]] y");
        assert_eq!(k, vec![
            Token::Name("x".into()),
            Token::Name("y".into()),
            Token::Eof
        ]);
    }

    #[test]
    fn tracks_positions() {
        let toks = lex("a\n  b").unwrap();
        assert_eq!(toks[0].1.line, 1);
        assert_eq!(toks[1].1.line, 2);
        assert_eq!(toks[1].1.col, 3);
    }

    #[test]
    fn reports_unterminated_string() {
        let err = lex("\"open").unwrap_err();
        assert!(err.message.contains("незакрытая строка"));
    }

    #[test]
    fn reports_bad_number() {
        assert!(lex("1.2.3").is_err());
    }
}
