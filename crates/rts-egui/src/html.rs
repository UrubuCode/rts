//! Tokenizador HTML MÍNIMO à mão (sem crate externa).
//!
//! É só a etapa LÉXICA: quebra a string em `<tag>` / `</tag>` / texto. A etapa
//! SINTÁTICA (montar a árvore de `Dom`) vive em `dom.rs`, que consome estes
//! tokens. Não é um parser conforme à spec — cobre o subconjunto necessário ao
//! P1 (tags simples, atributos descartados, 3 entidades básicas).
//!
//! Robustez: atributos são ignorados, o nome da tag é normalizado para
//! minúsculas, e `&amp;`/`&lt;`/`&gt;` são decodificados no texto.

/// Um token cru do HTML: ou uma tag (com flag de fechamento) ou texto literal.
///
/// `pub(crate)` para que `dom.rs` reuse o mesmo tokenizador (uma única fonte de
/// verdade da etapa léxica; o parser de árvore difere só na etapa sintática).
pub(crate) enum Token {
    /// `<nome ...>` (atributos descartados) — `close=true` para `</nome>`.
    Tag { name: String, close: bool },
    /// Texto entre tags, já com entidades decodificadas.
    Text(String),
}

/// Tokeniza o HTML char a char. Ao ver `<`, lê até `>`; senão acumula texto.
pub(crate) fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    let mut text = String::new();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Fecha o texto acumulado antes da tag.
            if !text.is_empty() {
                tokens.push(Token::Text(decode_entities(&text)));
                text.clear();
            }
            // Lê até o `>` (ou fim da string, defensivo).
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            let raw = &html[start..j.min(html.len())];
            i = if j < bytes.len() { j + 1 } else { j };

            let close = raw.starts_with('/');
            let raw = raw.trim_start_matches('/').trim();
            // Nome = primeiro token (antes de espaço/atributos), em minúsculas.
            let name = raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if !name.is_empty() {
                tokens.push(Token::Tag { name, close });
            }
        } else {
            // Acumula char de texto (respeitando UTF-8: copia o char inteiro).
            let ch = html[i..].chars().next().unwrap();
            text.push(ch);
            i += ch.len_utf8();
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(decode_entities(&text)));
    }
    tokens
}

/// Decodifica as 3 entidades básicas do P1. Substituímos as três de uma vez, sem
/// risco de dupla-decodificação.
fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
