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
    /// `<nome attrs...>` — `close=true` para `</nome>`. `attrs_raw` é a parte
    /// crua após o nome (`class='x' id='y'`), vazia em tags de fechamento; a
    /// etapa sintática (`dom.rs`) a parseia em pares. Mantemos cru aqui para o
    /// tokenizador continuar só-léxico.
    Tag { name: String, attrs_raw: String, close: bool },
    /// Texto entre tags, já com entidades decodificadas.
    Text(String),
    /// Comentário `<!-- ... -->`. Conteúdo CRU entre os delimitadores (sem
    /// decodificar entidades — comentário é texto literal). Vira `NodeKind::Comment`
    /// na etapa sintática (DOM fiel preserva comentários).
    Comment(String),
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
            // Comentário `<!-- ... -->`: o `>` pode aparecer DENTRO, então o fim é
            // o `-->`, não o primeiro `>`. Trata antes do caminho de tag normal.
            if html[i..].starts_with("<!--") {
                let body_start = i + 4;
                let rest = &html[body_start..];
                let (content, advance) = match rest.find("-->") {
                    Some(end) => (&rest[..end], 4 + end + 3), // `<!--` + corpo + `-->`
                    None => (rest, html.len() - i),            // sem fechar: vai até o fim
                };
                tokens.push(Token::Comment(content.to_string()));
                i += advance;
                continue;
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
            // Tag autofechável `<br/>`: tira a `/` final também.
            let raw = raw.trim_start_matches('/').trim_end_matches('/').trim();
            // Nome = primeiro token (antes de espaço/atributos), em minúsculas;
            // attrs_raw = o que sobra após o nome (só em tags de abertura).
            let mut parts = raw.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("").to_ascii_lowercase();
            let attrs_raw = if close {
                String::new()
            } else {
                parts.next().unwrap_or("").trim().to_string()
            };
            if !name.is_empty() {
                tokens.push(Token::Tag { name, attrs_raw, close });
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

/// Decodifica entidades HTML num único passe (sem `.replace` encadeado, que não
/// pega as numéricas e arrisca dupla-decodificação). Cobre as nomeadas comuns
/// (`&lt; &gt; &amp; &quot; &apos; &nbsp;`) e as numéricas decimais (`&#NN;`) e
/// hex (`&#xNN;`). Uma entidade desconhecida ou malformada é deixada literal —
/// robustez de parser real. `pub(crate)` — reusada por `dom.rs` ao decodar
/// valores de atributo.
pub(crate) fn decode_entities(s: &str) -> String {
    // Atalho: sem `&`, nada a decodificar (caso comum).
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < s.len() {
        if bytes[i] != b'&' {
            // Copia o char inteiro (UTF-8-safe).
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        // Acha o `;` de fechamento numa janela curta (entidades são curtas).
        match s[i + 1..].find(';').filter(|&rel| rel <= 10) {
            Some(rel) => {
                let body = &s[i + 1..i + 1 + rel];
                if let Some(ch) = decode_one_entity(body) {
                    out.push(ch);
                    i += 1 + rel + 1; // pula `&body;`
                    continue;
                }
                // Desconhecida: deixa o `&` literal e segue.
                out.push('&');
                i += 1;
            }
            None => {
                // `&` solto sem `;`: literal.
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

/// Decodifica o MIOLO de uma entidade (sem o `&` e o `;`). `None` se desconhecida.
fn decode_one_entity(body: &str) -> Option<char> {
    if let Some(num) = body.strip_prefix('#') {
        // Numérica: `#NN` decimal ou `#xNN`/`#XNN` hex.
        let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        return char::from_u32(code);
    }
    Some(match body {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entidades_nomeadas() {
        assert_eq!(decode_entities("a &lt; b &gt; c &amp; d"), "a < b > c & d");
        assert_eq!(decode_entities("&quot;aspas&quot; &apos;simples&apos;"), "\"aspas\" 'simples'");
        assert_eq!(decode_entities("x&nbsp;y"), "x\u{00A0}y");
    }

    #[test]
    fn entidades_numericas() {
        assert_eq!(decode_entities("&#65;&#66;&#67;"), "ABC"); // decimal
        assert_eq!(decode_entities("&#x41;&#x42;"), "AB"); // hex minúsculo
        assert_eq!(decode_entities("&#X41;"), "A"); // hex maiúsculo
        assert_eq!(decode_entities("caf&#233;"), "café"); // não-ASCII decimal
        assert_eq!(decode_entities("&#9731;"), "☃"); // BMP fora do Latin-1
    }

    #[test]
    fn malformadas_ficam_literais() {
        assert_eq!(decode_entities("Tom & Jerry"), "Tom & Jerry"); // `&` solto
        assert_eq!(decode_entities("&naoexiste;"), "&naoexiste;"); // nome desconhecido
        assert_eq!(decode_entities("&#abc;"), "&#abc;"); // numérica inválida
        assert_eq!(decode_entities("100% & mais"), "100% & mais");
        assert_eq!(decode_entities("sem ampersand"), "sem ampersand"); // atalho sem `&`
    }

    #[test]
    fn entidade_no_fim_e_consecutivas() {
        assert_eq!(decode_entities("fim &amp;"), "fim &");
        assert_eq!(decode_entities("&lt;&lt;&gt;&gt;"), "<<>>");
    }
}
