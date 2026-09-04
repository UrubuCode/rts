//! Sintaxe CSS preservada entre o texto de entrada e o IR da cascade.
//!
//! Esta camada não conhece `ComputedStyle`, layout ou seletores. Ela existe para
//! separar a gramática CSS do lowering semântico: tokens e nós desconhecidos são
//! preservados, enquanto camadas posteriores decidem o que o motor sabe aplicar.

/// Intervalo de bytes no CSS original (`start` inclusivo, `end` exclusivo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub span: SourceSpan,
    pub message: String,
}

/// Tokens CSS com o texto cru preservado no `Token::raw`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Whitespace,
    Comment,
    Ident(String),
    AtKeyword(String),
    Hash(String),
    String(String),
    Number(String),
    Percentage(String),
    Dimension {
        number: String,
        unit: String,
    },
    Colon,
    Semicolon,
    Comma,
    OpenCurly,
    CloseCurly,
    OpenParen,
    CloseParen,
    OpenSquare,
    CloseSquare,
    /// O nome já inclui o `(`, como no `function-token` da CSS Syntax.
    Function(String),
    Delim(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
    /// Substring exacta da entrada, incluindo escapes e whitespace.
    pub raw: String,
}

impl Token {
    fn new(kind: TokenKind, source: &str, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: SourceSpan::new(start, end),
            raw: source[start..end].to_string(),
        }
    }

    fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Whitespace | TokenKind::Comment)
    }
}

fn next_char(source: &str, at: usize) -> Option<(char, usize)> {
    source.get(at..)?.chars().next().map(|c| (c, c.len_utf8()))
}

fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ')
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

fn is_name_start(c: char) -> bool {
    c == '_' || c == '-' || c.is_ascii_alphabetic() || !c.is_ascii()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || is_digit(c)
}

fn starts_ident(source: &str, at: usize) -> bool {
    let Some((first, first_len)) = next_char(source, at) else {
        return false;
    };
    if first == '-' {
        let Some((second, _)) = next_char(source, at + first_len) else {
            return false;
        };
        return is_name_start(second) || second == '-';
    }
    is_name_start(first) && first != '-'
}

fn starts_number(source: &str, at: usize) -> bool {
    let Some((first, first_len)) = next_char(source, at) else {
        return false;
    };
    let next = next_char(source, at + first_len).map(|(c, _)| c);
    let next2 = next
        .and_then(|_| next_char(source, at + first_len + next.unwrap().len_utf8()))
        .map(|(c, _)| c);
    if is_digit(first) {
        return true;
    }
    if matches!(first, '+' | '-') {
        return next.is_some_and(is_digit) || (next == Some('.') && next2.is_some_and(is_digit));
    }
    first == '.' && next.is_some_and(is_digit)
}

fn consume_escape(source: &str, at: usize) -> (String, usize) {
    let Some((slash, slash_len)) = next_char(source, at) else {
        return (String::new(), at);
    };
    if slash != '\\' {
        return (slash.to_string(), at + slash_len);
    }
    let mut i = at + slash_len;
    let Some((first, first_len)) = next_char(source, i) else {
        return (String::new(), i);
    };
    if is_whitespace(first) {
        return (String::new(), i + first_len);
    }
    if is_hex(first) {
        let mut hex = String::new();
        let mut count = 0;
        while count < 6 {
            let Some((c, len)) = next_char(source, i) else {
                break;
            };
            if !is_hex(c) {
                break;
            }
            hex.push(c);
            i += len;
            count += 1;
        }
        if let Some((c, len)) = next_char(source, i) {
            if is_whitespace(c) {
                i += len;
            }
        }
        let value = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
        let decoded = char::from_u32(value).unwrap_or('\u{FFFD}');
        return (decoded.to_string(), i);
    }
    i += first_len;
    (first.to_string(), i)
}

fn consume_name(source: &str, at: usize) -> (String, usize) {
    let mut i = at;
    let mut decoded = String::new();
    while let Some((c, len)) = next_char(source, i) {
        if c == '\\' {
            let (part, next) = consume_escape(source, i);
            decoded.push_str(&part);
            i = next;
        } else if is_name_char(c) {
            decoded.push(c);
            i += len;
        } else {
            break;
        }
    }
    (decoded, i)
}

fn consume_number(source: &str, at: usize) -> usize {
    let mut i = at;
    if matches!(next_char(source, i).map(|(c, _)| c), Some('+' | '-')) {
        i += 1;
    }
    while next_char(source, i).is_some_and(|(c, _)| is_digit(c)) {
        i += 1;
    }
    if next_char(source, i).map(|(c, _)| c) == Some('.')
        && next_char(source, i + 1).is_some_and(|(c, _)| is_digit(c))
    {
        i += 1;
        while next_char(source, i).is_some_and(|(c, _)| is_digit(c)) {
            i += 1;
        }
    }
    if matches!(next_char(source, i).map(|(c, _)| c), Some('e' | 'E')) {
        let e = i;
        i += 1;
        if matches!(next_char(source, i).map(|(c, _)| c), Some('+' | '-')) {
            i += 1;
        }
        let digits = i;
        while next_char(source, i).is_some_and(|(c, _)| is_digit(c)) {
            i += 1;
        }
        if digits == i {
            i = e;
        }
    }
    i
}

/// Tokeniza CSS sem descartar comentários, escapes ou texto desconhecido.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let start = i;
        let Some((c, len)) = next_char(source, i) else {
            break;
        };
        if is_whitespace(c) {
            i += len;
            while next_char(source, i).is_some_and(|(ch, _)| is_whitespace(ch)) {
                i += next_char(source, i).unwrap().1;
            }
            tokens.push(Token::new(TokenKind::Whitespace, source, start, i));
            continue;
        }
        if source[start..].starts_with("/*") {
            i += 2;
            while i < source.len() && !source[i..].starts_with("*/") {
                i += next_char(source, i).map(|(_, n)| n).unwrap_or(1);
            }
            if i < source.len() {
                i += 2;
            }
            tokens.push(Token::new(
                TokenKind::Comment,
                source,
                start,
                i.min(source.len()),
            ));
            continue;
        }
        if matches!(c, '\'' | '"') {
            let quote = c;
            i += len;
            let mut value = String::new();
            while let Some((ch, ch_len)) = next_char(source, i) {
                if ch == quote {
                    i += ch_len;
                    break;
                }
                if ch == '\\' {
                    let (part, next) = consume_escape(source, i);
                    value.push_str(&part);
                    i = next;
                } else {
                    value.push(ch);
                    i += ch_len;
                }
            }
            tokens.push(Token::new(TokenKind::String(value), source, start, i));
            continue;
        }
        if starts_number(source, i) {
            let number_end = consume_number(source, i);
            let number = source[start..number_end].to_string();
            i = number_end;
            if next_char(source, i).map(|(ch, _)| ch) == Some('%') {
                i += 1;
                tokens.push(Token::new(TokenKind::Percentage(number), source, start, i));
            } else if starts_ident(source, i) {
                let (unit, end) = consume_name(source, i);
                i = end;
                tokens.push(Token::new(
                    TokenKind::Dimension { number, unit },
                    source,
                    start,
                    i,
                ));
            } else {
                tokens.push(Token::new(TokenKind::Number(number), source, start, i));
            }
            continue;
        }
        if c == '@' && starts_ident(source, start + len) {
            let (name, end) = consume_name(source, start + len);
            i = end;
            tokens.push(Token::new(TokenKind::AtKeyword(name), source, start, i));
            continue;
        }
        if c == '#' && starts_ident(source, start + len) {
            let (name, end) = consume_name(source, start + len);
            i = end;
            tokens.push(Token::new(TokenKind::Hash(name), source, start, i));
            continue;
        }
        if starts_ident(source, i) {
            let (name, end) = consume_name(source, i);
            i = end;
            if next_char(source, i).map(|(ch, _)| ch) == Some('(') {
                i += 1;
                tokens.push(Token::new(TokenKind::Function(name), source, start, i));
            } else {
                tokens.push(Token::new(TokenKind::Ident(name), source, start, i));
            }
            continue;
        }
        i += len;
        let kind = match c {
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            '{' => TokenKind::OpenCurly,
            '}' => TokenKind::CloseCurly,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '[' => TokenKind::OpenSquare,
            ']' => TokenKind::CloseSquare,
            _ => TokenKind::Delim(c),
        };
        tokens.push(Token::new(kind, source, start, i));
    }
    tokens
}

/// Um valor composto preserva blocos aninhados (`calc()`, `[]`, `{}`) sem os
/// achatar em texto. `Token` continua disponível para extensões futuras.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentValue {
    Token(Token),
    Function {
        name: String,
        /// Texto original do nome mais `(`, incluindo escapes.
        raw_open: String,
        arguments: Vec<ComponentValue>,
        /// `None` quando a função não tinha `)` de fecho.
        close_raw: Option<String>,
        span: SourceSpan,
    },
    SimpleBlock {
        open: char,
        /// Delimitador original, preservado para serialização lossless.
        open_raw: String,
        values: Vec<ComponentValue>,
        close: Option<char>,
        close_raw: Option<String>,
        span: SourceSpan,
    },
}

impl ComponentValue {
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Token(token) => token.span,
            Self::Function { span, .. } | Self::SimpleBlock { span, .. } => *span,
        }
    }

    /// Reconstrói o texto original do componente, preservando whitespace e
    /// comentários dos tokens que ainda não foram interpretados.
    pub fn to_css(&self) -> String {
        match self {
            Self::Token(token) => token.raw.clone(),
            Self::Function {
                raw_open,
                arguments,
                close_raw,
                ..
            } => {
                let mut out = raw_open.clone();
                for value in arguments {
                    out.push_str(&value.to_css());
                }
                if let Some(raw) = close_raw {
                    out.push_str(raw);
                }
                out
            }
            Self::SimpleBlock {
                open,
                open_raw,
                values,
                close,
                close_raw,
                ..
            } => {
                let close_char = close.unwrap_or(match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    _ => *open,
                });
                let mut out = open_raw.clone();
                for value in values {
                    out.push_str(&value.to_css());
                }
                if let Some(raw) = close_raw {
                    out.push_str(raw);
                } else {
                    out.push(close_char);
                }
                out
            }
        }
    }

    /// Reconstrói o componente para consumo semântico, removendo apenas
    /// comentários CSS. Whitespace, strings, escapes e funções permanecem.
    pub fn to_css_semantic(&self) -> String {
        match self {
            Self::Token(token) if matches!(token.kind, TokenKind::Comment) => String::new(),
            Self::Token(token) => token.raw.clone(),
            Self::Function {
                raw_open,
                arguments,
                close_raw,
                ..
            } => {
                let mut out = raw_open.clone();
                for value in arguments {
                    out.push_str(&value.to_css_semantic());
                }
                if let Some(raw) = close_raw {
                    out.push_str(raw);
                }
                out
            }
            Self::SimpleBlock {
                open,
                open_raw,
                values,
                close,
                close_raw,
                ..
            } => {
                let close_char = close.unwrap_or(match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    _ => *open,
                });
                let mut out = open_raw.clone();
                for value in values {
                    out.push_str(&value.to_css_semantic());
                }
                if let Some(raw) = close_raw {
                    out.push_str(raw);
                } else if close.is_some() {
                    out.push(close_char);
                }
                out
            }
        }
    }

    pub(crate) fn is_trivia(&self) -> bool {
        matches!(self, Self::Token(token) if token.is_trivia())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockAst {
    pub values: Vec<ComponentValue>,
    pub span: SourceSpan,
    /// `false` quando o source terminou sem o delimitador de fecho.
    pub closed: bool,
    /// Filhos estruturais de at-rules cujo bloco contém regras, como `@media`.
    /// Blocos de declarações mantêm `None` e usam `values` directamente.
    pub nested: Option<Box<StylesheetAst>>,
}

impl BlockAst {
    /// Divide o bloco em declarações de topo sem perder valores complexos ou
    /// declarações desconhecidas. Regras aninhadas continuam acessíveis em
    /// `values` e não são falsamente interpretadas como propriedades.
    pub fn declarations(&self) -> Vec<DeclarationAst> {
        let mut output = Vec::new();
        let mut current = Vec::new();
        for value in &self.values {
            let semicolon = matches!(
                value,
                ComponentValue::Token(Token {
                    kind: TokenKind::Semicolon,
                    ..
                })
            );
            if semicolon {
                if let Some(declaration) = declaration_from_values(&current) {
                    output.push(declaration);
                }
                current.clear();
            } else {
                current.push(value.clone());
            }
        }
        if let Some(declaration) = declaration_from_values(&current) {
            output.push(declaration);
        }
        output
    }

    pub fn to_css(&self) -> String {
        self.values.iter().map(ComponentValue::to_css).collect()
    }

    pub fn to_css_semantic(&self) -> String {
        self.values
            .iter()
            .map(ComponentValue::to_css_semantic)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationAst {
    /// Nome descodificado para o lowering semântico; a case original permanece
    /// disponível em `name_raw`.
    pub name: String,
    /// Grafia original do nome, incluindo escapes e whitespace.
    pub name_raw: String,
    pub value: Vec<ComponentValue>,
    pub important: bool,
    pub span: SourceSpan,
}

impl DeclarationAst {
    pub fn value_css(&self) -> String {
        let mut start = 0;
        let mut end = self.value.len();
        while start < end && self.value[start].is_trivia() {
            start += 1;
        }
        while end > start && self.value[end - 1].is_trivia() {
            end -= 1;
        }
        self.value[start..end]
            .iter()
            .map(ComponentValue::to_css)
            .collect()
    }
}

/// Declarações no estado especificado: a fonte já foi tokenizada, mas ainda não
/// foi convertida para valores computados nem resolvida contra o elemento/pai.
///
/// Esta camada é deliberadamente pequena. O `ComputedStyle` continua a ser o
/// formato eficiente usado pela cascade e pelo layout; este tipo é a fronteira
/// estável para tooling, diagnósticos e futuros parsers de valores por
/// propriedade.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpecifiedStyle {
    pub declarations: std::rc::Rc<[DeclarationAst]>,
}

impl SpecifiedStyle {
    pub fn from_block(block: &BlockAst) -> Self {
        Self {
            declarations: block.declarations().into_boxed_slice().into(),
        }
    }

    pub fn declarations(&self) -> &[DeclarationAst] {
        &self.declarations
    }

    /// Serialização normalizada das declarações especificadas. A serialização
    /// lossless do stylesheet completo continua em `StylesheetAst::to_css()`.
    pub fn to_css(&self) -> String {
        self.declarations
            .iter()
            .map(|declaration| {
                let important = if declaration.important {
                    " !important"
                } else {
                    ""
                };
                format!(
                    "{}: {}{};",
                    declaration.name_raw.trim(),
                    declaration.value_css(),
                    important
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstItem {
    QualifiedRule {
        prelude: Vec<ComponentValue>,
        block: BlockAst,
        span: SourceSpan,
    },
    AtRule {
        name: String,
        prelude: Vec<ComponentValue>,
        block: Option<BlockAst>,
        span: SourceSpan,
    },
    /// Texto estrutural que não forma uma regra válida, preservado para
    /// diagnósticos em vez de desaparecer durante o parse.
    Invalid {
        values: Vec<ComponentValue>,
        span: SourceSpan,
    },
}

impl AstItem {
    pub fn prelude_css(&self) -> String {
        match self {
            Self::QualifiedRule { prelude, .. } | Self::AtRule { prelude, .. } => prelude
                .iter()
                .map(ComponentValue::to_css_semantic)
                .collect(),
            Self::Invalid { values, .. } => values.iter().map(ComponentValue::to_css).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StylesheetAst {
    pub items: Vec<AstItem>,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

impl StylesheetAst {
    /// Reconstrói exactamente os bytes tokenizados da entrada, incluindo
    /// comentários e whitespace.
    pub fn to_css(&self) -> String {
        self.tokens.iter().map(|token| token.raw.as_str()).collect()
    }

    pub fn parse(source: &str) -> Self {
        let tokens = tokenize(source);
        let mut parser = Parser {
            tokens: &tokens,
            at: 0,
        };
        let items = parser.parse_rule_list(None);
        let diagnostics = collect_diagnostics(&items);
        Self {
            items,
            tokens,
            diagnostics,
        }
    }
}

fn collect_diagnostics(items: &[AstItem]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for item in items {
        match item {
            AstItem::Invalid { span, .. } => diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Error,
                span: *span,
                message: "estrutura CSS inválida ou incompleta".to_string(),
            }),
            AstItem::QualifiedRule { block, .. } | AstItem::AtRule { block: Some(block), .. } => {
                if !block.closed {
                    diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Error,
                        span: block.span,
                        message: "bloco CSS sem chave de fecho".to_string(),
                    });
                }
                collect_component_diagnostics(&block.values, &mut diagnostics);
            }
            AstItem::AtRule { block: None, .. } => {}
        }
    }
    diagnostics
}

fn collect_component_diagnostics(
    values: &[ComponentValue],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for value in values {
        match value {
            ComponentValue::Function {
                close_raw: None, span, ..
            } => diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Error,
                span: *span,
                message: "função CSS sem parêntese de fecho".to_string(),
            }),
            ComponentValue::Function { arguments, .. } => {
                collect_component_diagnostics(arguments, diagnostics)
            }
            ComponentValue::SimpleBlock {
                close: None,
                span,
                values,
                ..
            } => {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    span: *span,
                    message: "bloco CSS sem delimitador de fecho".to_string(),
                });
                collect_component_diagnostics(values, diagnostics);
            }
            ComponentValue::SimpleBlock { values, .. } => {
                collect_component_diagnostics(values, diagnostics)
            }
            ComponentValue::Token(_) => {}
        }
    }
}

fn token_is_colon(value: &ComponentValue) -> bool {
    matches!(
        value,
        ComponentValue::Token(Token {
            kind: TokenKind::Colon,
            ..
        })
    )
}

fn declaration_from_values(values: &[ComponentValue]) -> Option<DeclarationAst> {
    let first = values.iter().position(|value| !value.is_trivia())?;
    let colon = values[first..].iter().position(token_is_colon)? + first;
    let name = crate::style::declaracao_nome::nome_da_declaracao(&values[first..colon])?;
    let mut value = values[colon + 1..].to_vec();
    let mut important = false;
    let mut significant = value.len();
    while significant > 0 && value[significant - 1].is_trivia() {
        significant -= 1;
    }
    if significant >= 2 {
        let bang = matches!(
            &value[significant - 2],
            ComponentValue::Token(Token {
                kind: TokenKind::Delim('!'),
                ..
            })
        );
        let keyword = matches!(
            &value[significant - 1],
            ComponentValue::Token(Token { kind: TokenKind::Ident(word), .. })
                if word.eq_ignore_ascii_case("important")
        );
        if bang && keyword {
            important = true;
            value.truncate(significant - 2);
            while value.last().is_some_and(ComponentValue::is_trivia) {
                value.pop();
            }
        }
    }
    let start = values[first].span().start;
    let end = values.last().map(|value| value.span().end).unwrap_or(start);
    let name_raw = values[first..colon]
        .iter()
        .map(ComponentValue::to_css)
        .collect();
    Some(DeclarationAst {
        name,
        name_raw,
        value,
        important,
        span: SourceSpan::new(start, end),
    })
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl<'a> Parser<'a> {
    fn skip_trivia(&mut self) {
        while self.at < self.tokens.len() && self.tokens[self.at].is_trivia() {
            self.at += 1;
        }
    }

    fn parse_rule_list(&mut self, until: Option<char>) -> Vec<AstItem> {
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            if self.at >= self.tokens.len() {
                break;
            }
            if until == Some('}') && matches!(self.tokens[self.at].kind, TokenKind::CloseCurly) {
                self.at += 1;
                break;
            }
            let start = self.tokens[self.at].span.start;
            let prelude = self.parse_prelude();
            match self.tokens.get(self.at).map(|token| &token.kind) {
                Some(TokenKind::Semicolon) => {
                    self.at += 1;
                    if let Some(name) = at_rule_name(&prelude) {
                        items.push(AstItem::AtRule {
                            name,
                            prelude: remove_at_keyword(prelude),
                            block: None,
                            span: SourceSpan::new(start, self.tokens[self.at - 1].span.end),
                        });
                    } else if !prelude.is_empty() {
                        items.push(AstItem::Invalid {
                            span: SourceSpan::new(start, self.tokens[self.at - 1].span.end),
                            values: prelude,
                        });
                    }
                }
                Some(TokenKind::OpenCurly) => {
                    let block = self.parse_block();
                    let end = block.span.end;
                    if let Some(name) = at_rule_name(&prelude) {
                        items.push(AstItem::AtRule {
                            name,
                            prelude: remove_at_keyword(prelude),
                            block: Some(block),
                            span: SourceSpan::new(start, end),
                        });
                    } else {
                        items.push(AstItem::QualifiedRule {
                            prelude,
                            block,
                            span: SourceSpan::new(start, end),
                        });
                    }
                }
                Some(TokenKind::CloseCurly) | None => {
                    if !prelude.is_empty() {
                        let end = prelude
                            .last()
                            .map(|value| value.span().end)
                            .unwrap_or(start);
                        items.push(AstItem::Invalid {
                            values: prelude,
                            span: SourceSpan::new(start, end),
                        });
                    }
                    if self.at < self.tokens.len()
                        && matches!(self.tokens[self.at].kind, TokenKind::CloseCurly)
                    {
                        self.at += 1;
                    }
                    break;
                }
                _ => {
                    if !prelude.is_empty() {
                        let end = prelude
                            .last()
                            .map(|value| value.span().end)
                            .unwrap_or(start);
                        items.push(AstItem::Invalid {
                            values: prelude,
                            span: SourceSpan::new(start, end),
                        });
                    } else {
                        self.at += 1;
                    }
                }
            }
        }
        items
    }

    fn parse_prelude(&mut self) -> Vec<ComponentValue> {
        let mut values = Vec::new();
        while self.at < self.tokens.len() {
            match self.tokens[self.at].kind {
                TokenKind::Semicolon | TokenKind::OpenCurly | TokenKind::CloseCurly => break,
                _ => values.push(self.parse_component_value()),
            }
        }
        values
    }

    fn parse_block(&mut self) -> BlockAst {
        let start = self.tokens[self.at].span.start;
        self.at += 1; // `{`
        let mut values = Vec::new();
        let mut closed = false;
        let mut end = self
            .tokens
            .get(self.at - 1)
            .map(|t| t.span.end)
            .unwrap_or(start);
        while self.at < self.tokens.len() {
            if matches!(self.tokens[self.at].kind, TokenKind::CloseCurly) {
                end = self.tokens[self.at].span.end;
                self.at += 1;
                closed = true;
                break;
            }
            let value = self.parse_component_value();
            end = value.span().end;
            values.push(value);
        }
        let has_nested_rules = values.iter().any(|value| {
            matches!(
                value,
                ComponentValue::SimpleBlock { open: '{', .. }
            )
        });
        let nested = has_nested_rules.then(|| {
            let source: String = values
                .iter()
                .map(ComponentValue::to_css)
                .collect();
            Box::new(StylesheetAst::parse(&source))
        });
        BlockAst {
            values,
            span: SourceSpan::new(start, end),
            closed,
            nested,
        }
    }

    fn parse_component_value(&mut self) -> ComponentValue {
        let token = self.tokens[self.at].clone();
        self.at += 1;
        match token.kind {
            TokenKind::Function(name) => {
                let start = token.span.start;
                let raw_open = token.raw.clone();
                let mut arguments = Vec::new();
                let mut close_raw = None;
                let mut end = token.span.end;
                while self.at < self.tokens.len() {
                    if matches!(self.tokens[self.at].kind, TokenKind::CloseParen) {
                        close_raw = Some(self.tokens[self.at].raw.clone());
                        end = self.tokens[self.at].span.end;
                        self.at += 1;
                        break;
                    }
                    let value = self.parse_component_value();
                    end = value.span().end;
                    arguments.push(value);
                }
                ComponentValue::Function {
                    name,
                    raw_open,
                    arguments,
                    close_raw,
                    span: SourceSpan::new(start, end),
                }
            }
            TokenKind::OpenParen => self.parse_simple_block(token, ')'),
            TokenKind::OpenSquare => self.parse_simple_block(token, ']'),
            TokenKind::OpenCurly => self.parse_simple_block(token, '}'),
            _ => ComponentValue::Token(token),
        }
    }

    fn parse_simple_block(&mut self, opening: Token, expected_close: char) -> ComponentValue {
        let open = match opening.kind {
            TokenKind::OpenParen => '(',
            TokenKind::OpenSquare => '[',
            TokenKind::OpenCurly => '{',
            _ => '?',
        };
        let mut values = Vec::new();
        let mut end = opening.span.end;
        let close_kind = match expected_close {
            ')' => |kind: &TokenKind| matches!(kind, TokenKind::CloseParen),
            ']' => |kind: &TokenKind| matches!(kind, TokenKind::CloseSquare),
            '}' => |kind: &TokenKind| matches!(kind, TokenKind::CloseCurly),
            _ => |_: &TokenKind| false,
        };
        let mut close = None;
        let mut close_raw = None;
        while self.at < self.tokens.len() {
            if close_kind(&self.tokens[self.at].kind) {
                close = Some(expected_close);
                close_raw = Some(self.tokens[self.at].raw.clone());
                end = self.tokens[self.at].span.end;
                self.at += 1;
                break;
            }
            let value = self.parse_component_value();
            end = value.span().end;
            values.push(value);
        }
        ComponentValue::SimpleBlock {
            open,
            open_raw: opening.raw,
            values,
            close,
            close_raw,
            span: SourceSpan::new(opening.span.start, end),
        }
    }
}

fn at_rule_name(values: &[ComponentValue]) -> Option<String> {
    values.iter().find_map(|value| match value {
        ComponentValue::Token(Token {
            kind: TokenKind::AtKeyword(name),
            ..
        }) => Some(name.clone()),
        _ => None,
    })
}

fn remove_at_keyword(values: Vec<ComponentValue>) -> Vec<ComponentValue> {
    let mut removed = false;
    values
        .into_iter()
        .filter(|value| {
            if !removed
                && matches!(
                    value,
                    ComponentValue::Token(Token {
                        kind: TokenKind::AtKeyword(_),
                        ..
                    })
                )
            {
                removed = true;
                false
            } else {
                true
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_preserva_spans_e_formas_css() {
        let css = "/* c */ .card { width: calc(100% - 8px); color: #abc; --x: \"a\\n\" }";
        let tokens = tokenize(css);
        assert!(matches!(tokens[0].kind, TokenKind::Comment));
        assert!(tokens.iter().any(|token| matches!(
            token.kind,
            TokenKind::Function(ref name) if name == "calc"
        )));
        assert!(tokens.iter().any(|token| matches!(
            token.kind,
            TokenKind::Percentage(ref value) if value == "100"
        )));
        assert!(tokens.iter().any(|token| matches!(
            token.kind,
            TokenKind::Dimension { ref number, ref unit } if number == "8" && unit == "px"
        )));
        for pair in tokens.windows(2) {
            assert!(pair[0].span.end <= pair[1].span.start);
        }
    }

    #[test]
    fn ast_preserva_regras_at_rules_e_declaracoes_desconhecidas() {
        let sheet = StylesheetAst::parse(
            "@media (min-width: 10px) { .card, .x { color: red !important; future-prop: mystery(1, 2); } }",
        );
        assert_eq!(sheet.items.len(), 1);
        let AstItem::AtRule { name, block, .. } = &sheet.items[0] else {
            panic!("esperava at-rule")
        };
        assert_eq!(name, "media");
        let block = block.as_ref().unwrap();
        assert!(!block.values.is_empty());
        let nested = block.nested.as_ref().expect("filhos estruturais");
        assert!(matches!(nested.items[0], AstItem::QualifiedRule { .. }));
        let nested_css = block.to_css();
        assert!(nested_css.contains("future-prop: mystery(1, 2)"));
    }

    #[test]
    fn declaracoes_separam_important_e_valor_complexo() {
        let sheet = StylesheetAst::parse(".a { color: red !important; width: calc(100% - 2px); }");
        let AstItem::QualifiedRule { block, .. } = &sheet.items[0] else {
            panic!("esperava regra")
        };
        let declarations = block.declarations();
        assert_eq!(declarations.len(), 2);
        assert!(declarations[0].important);
        assert_eq!(declarations[0].name, "color");
        assert_eq!(declarations[0].name_raw, "color");
        assert_eq!(declarations[1].value_css(), "calc(100% - 2px)");
    }
}
