use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Number(f64),
    String(String),
    True,
    False,
    Null,
    Undefined,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    EqualEqual,
    BangEqual,
    StrictEqual,
    StrictNotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    AndAnd,
    OrOr,
    QuestionQuestion,
    LeftParen,
    RightParen,
    Comma,
    Dot,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub index: usize,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];

        if current.is_whitespace() {
            index += 1;
            continue;
        }

        if current.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }

            if index < chars.len() && chars[index] == '.' {
                index += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
            }

            let lexeme = chars[start..index].iter().collect::<String>();
            let number = lexeme.parse::<f64>()?;
            tokens.push(Token {
                kind: TokenKind::Number(number),
                index: start,
            });
            continue;
        }

        if is_identifier_start(current) {
            let start = index;
            index += 1;
            while index < chars.len() && is_identifier_continue(chars[index]) {
                index += 1;
            }

            let identifier = chars[start..index].iter().collect::<String>();
            let kind = match identifier.as_str() {
                "true" => TokenKind::True,
                "false" => TokenKind::False,
                "null" => TokenKind::Null,
                "undefined" => TokenKind::Undefined,
                _ => TokenKind::Identifier(identifier),
            };

            tokens.push(Token { kind, index: start });
            continue;
        }

        if current == '"' || current == '\'' || current == '`' {
            let quote = current;
            let start = index;
            index += 1;
            let mut value = String::new();
            let mut escape = false;
            let mut closed = false;

            while index < chars.len() {
                let ch = chars[index];
                index += 1;

                if escape {
                    let escaped = match ch {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '\'' => '\'',
                        '"' => '"',
                        '`' => '`',
                        other => other,
                    };
                    value.push(escaped);
                    escape = false;
                    continue;
                }

                if ch == '\\' {
                    escape = true;
                    continue;
                }

                if ch == quote {
                    closed = true;
                    break;
                }

                value.push(ch);
            }

            if !closed {
                bail!("unterminated string literal");
            }

            tokens.push(Token {
                kind: TokenKind::String(value),
                index: start,
            });
            continue;
        }

        let token = if index + 2 < chars.len() {
            match (chars[index], chars[index + 1], chars[index + 2]) {
                ('=', '=', '=') => {
                    index += 3;
                    Some(TokenKind::StrictEqual)
                }
                ('!', '=', '=') => {
                    index += 3;
                    Some(TokenKind::StrictNotEqual)
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(kind) = token {
            tokens.push(Token { kind, index });
            continue;
        }

        let two_char = if index + 1 < chars.len() {
            match (chars[index], chars[index + 1]) {
                ('=', '=') => {
                    index += 2;
                    Some(TokenKind::EqualEqual)
                }
                ('!', '=') => {
                    index += 2;
                    Some(TokenKind::BangEqual)
                }
                ('>', '=') => {
                    index += 2;
                    Some(TokenKind::GreaterThanOrEqual)
                }
                ('<', '=') => {
                    index += 2;
                    Some(TokenKind::LessThanOrEqual)
                }
                ('&', '&') => {
                    index += 2;
                    Some(TokenKind::AndAnd)
                }
                ('|', '|') => {
                    index += 2;
                    Some(TokenKind::OrOr)
                }
                ('?', '?') => {
                    index += 2;
                    Some(TokenKind::QuestionQuestion)
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(kind) = two_char {
            tokens.push(Token { kind, index });
            continue;
        }

        let kind = match chars[index] {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '!' => TokenKind::Bang,
            '>' => TokenKind::GreaterThan,
            '<' => TokenKind::LessThan,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            other => bail!("unsupported expression token '{}'", other),
        };

        tokens.push(Token { kind, index });
        index += 1;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        index: input.len(),
    });

    Ok(tokens)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}
