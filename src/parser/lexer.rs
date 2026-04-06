#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Number,
    StringLiteral,
    KeywordImport,
    KeywordInterface,
    KeywordClass,
    KeywordFunction,
    KeywordPublic,
    KeywordPrivate,
    KeywordProtected,
    KeywordReadonly,
    Symbol(char),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub line: usize,
    pub column: usize,
}

pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let mut cursor = 0usize;

        for fragment in line.split_whitespace() {
            if let Some(found) = line[cursor..].find(fragment) {
                let start = cursor + found;
                let column = start + 1;
                cursor = start + fragment.len();

                tokens.push(Token {
                    kind: classify(fragment),
                    lexeme: fragment.to_string(),
                    line: line_index + 1,
                    column,
                });
            }
        }
    }

    tokens
}

fn classify(fragment: &str) -> TokenKind {
    if fragment == "import" {
        return TokenKind::KeywordImport;
    }

    if fragment == "interface" {
        return TokenKind::KeywordInterface;
    }

    if fragment == "class" {
        return TokenKind::KeywordClass;
    }

    if fragment == "function" {
        return TokenKind::KeywordFunction;
    }

    if fragment == "public" {
        return TokenKind::KeywordPublic;
    }

    if fragment == "private" {
        return TokenKind::KeywordPrivate;
    }

    if fragment == "protected" {
        return TokenKind::KeywordProtected;
    }

    if fragment == "readonly" {
        return TokenKind::KeywordReadonly;
    }

    if fragment.starts_with('"') && fragment.ends_with('"') && fragment.len() >= 2 {
        return TokenKind::StringLiteral;
    }

    if fragment.chars().all(|c| c.is_ascii_digit()) {
        return TokenKind::Number;
    }

    if fragment.len() == 1 {
        if let Some(ch) = fragment.chars().next() {
            if "{}()[];,:".contains(ch) {
                return TokenKind::Symbol(ch);
            }
        }
    }

    if fragment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
    {
        return TokenKind::Identifier;
    }

    TokenKind::Unknown
}
