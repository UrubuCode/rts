//! Tokenizador HTML MÍNIMO à mão (sem crate externa).
//!
//! É só a etapa LÉXICA: quebra a string em `<tag>` / `</tag>` / texto /
//! comentário / raw-text. A etapa SINTÁTICA (montar a árvore de `Dom`) vive em
//! `dom.rs`, que consome estes tokens. Não é um parser conforme à spec — cobre
//! o subconjunto que páginas reais exigem.
//!
//! Robustez: nome da tag normalizado para minúsculas; atributos preservados
//! crus (`attrs_raw`, parseados em `dom.rs`); `>` dentro de valor de atributo
//! com aspas não fecha a tag; `<!DOCTYPE …>` e declarações `<!…>` são ignorados
//! (não modelamos DocumentType); entidades nomeadas comuns + numéricas
//! decodificadas no texto.

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
    /// Um elemento de TEXTO CRU (`<style>`/`<script>`): o conteúdo entre a abertura
    /// e o `</tag>` NÃO é HTML e não pode ser tokenizado como tags (CSS tem `{`,
    /// `>` em `a > b`; JS tem `<`). É lido literal até o fechamento casado. O
    /// parser de árvore decide o que fazer (CSS → stylesheet; script → ignorado).
    /// `attrs` preserva os atributos da tag de abertura (`<script src=…>`,
    /// `<style media=…>`) — necessários para resolver `<script src>`/`<link>`.
    RawElement { tag: String, attrs: String, content: String },
}

/// Tags cujo conteúdo é TEXTO CRU (não-HTML): `<style>` (CSS) e `<script>` (JS).
/// O tokenizer lê o miolo literal até `</tag>`, sem interpretar `<`/`{`/`>`.
fn is_raw_text_tag(tag: &str) -> bool {
    matches!(tag, "style" | "script")
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
            // Lê até o `>` que FECHA a tag (ou fim da string, defensivo),
            // respeitando aspas: um `>` DENTRO de valor de atributo com aspas
            // (`<div title="a>b">`) não termina a tag — parar no primeiro `>`
            // cru quebrava o atributo no meio e vazava `b">` como texto.
            // Aspas simples e duplas contam; são ASCII, então o scan por byte
            // continua UTF-8-safe.
            let start = i + 1;
            let mut j = start;
            let mut quote = 0u8; // 0 = fora de aspas; senão o byte da aspa aberta
            while j < bytes.len() {
                let b = bytes[j];
                if quote == 0 {
                    if b == b'>' {
                        break;
                    }
                    if b == b'"' || b == b'\'' {
                        quote = b;
                    }
                } else if b == quote {
                    quote = 0;
                }
                j += 1;
            }
            let raw = &html[start..j.min(html.len())];
            i = if j < bytes.len() { j + 1 } else { j };

            // Declaração de markup `<!...>` (`<!DOCTYPE html>` em qualquer caixa,
            // CDATA etc.): IGNORA — nenhum token é emitido. Não modelamos o nó
            // DocumentType (nodeType 10, fora do escopo); antes o doctype virava
            // `Element { tag: "!doctype" }` que EMPILHAVA na pilha de abertos
            // (a "tag" nunca fecha) e o documento inteiro aninhava como filho
            // dele. Comentário `<!--` já foi tratado acima.
            if raw.starts_with('!') {
                continue;
            }

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
                // `<style>`/`<script>`: o conteúdo é texto cru. Em vez de empurrar a
                // tag e tokenizar o miolo como HTML, consome literal até `</tag>`
                // (case-insensitive) e emite um único `RawElement`. Tags de
                // fechamento e autofecháveis (`<style/>`) não entram aqui.
                if !close && is_raw_text_tag(&name) && !raw.ends_with('/') {
                    let close_tag = format!("</{name}>");
                    let lower = html[i..].to_ascii_lowercase();
                    let (content, advance) = match lower.find(&close_tag) {
                        Some(end) => (&html[i..i + end], end + close_tag.len()),
                        None => (&html[i..], html.len() - i), // sem fechar: até o fim.
                    };
                    tokens.push(Token::RawElement { tag: name, attrs: attrs_raw, content: content.to_string() });
                    i += advance;
                    continue;
                }
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

/// Re-encoda um texto para conteúdo HTML (inverso de `decode_entities` no caminho
/// de TEXTO): `&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`. É o que `innerHTML` (GET) usa
/// para serializar um nó de texto de forma segura. `pub(crate)`.
pub(crate) fn encode_text_entities(s: &str) -> String {
    // Atalho: sem caractere especial, nada a fazer (caso comum).
    if !s.contains(['&', '<', '>']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Re-encoda um valor de ATRIBUTO (serializado entre aspas duplas): além de
/// `&`/`<`/`>`, escapa `"`→`&quot;` (o delimitador). Para `innerHTML`/`outerHTML`.
pub(crate) fn encode_attr_entities(s: &str) -> String {
    if !s.contains(['&', '<', '>', '"']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
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
        // As nomeadas mais comuns em páginas reais (o rodapé do google usa
        // `&copy;`; conteúdo editorial usa travessões/aspas tipográficas).
        "copy" => '\u{00A9}',
        "reg" => '\u{00AE}',
        "trade" => '\u{2122}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "middot" => '\u{00B7}',
        "bull" => '\u{2022}',
        "laquo" => '\u{00AB}',
        "raquo" => '\u{00BB}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201C}',
        "rdquo" => '\u{201D}',
        "deg" => '\u{00B0}',
        "times" => '\u{00D7}',
        "divide" => '\u{00F7}',
        "plusmn" => '\u{00B1}',
        "sect" => '\u{00A7}',
        "para" => '\u{00B6}',
        "euro" => '\u{20AC}',
        "pound" => '\u{00A3}',
        "yen" => '\u{00A5}',
        "cent" => '\u{00A2}',
        "shy" => '\u{00AD}',
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
        assert_eq!(decode_entities("&copy; 2026 &mdash; ok&hellip;"), "\u{00A9} 2026 \u{2014} ok\u{2026}");
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
    fn style_e_script_viram_raw_element() {
        // O conteúdo de <style>/<script> não é tokenizado como HTML: `{`, `>` em
        // `a > b`, `<` em script ficam literais até `</tag>`.
        let toks = tokenize("<style>.card { color: red } a > b {}</style><p>oi</p>");
        match &toks[0] {
            Token::RawElement { tag, content, .. } => {
                assert_eq!(tag, "style");
                assert_eq!(content, ".card { color: red } a > b {}");
            }
            _ => panic!("esperava RawElement no primeiro token"),
        }
        // o <p> depois é tokenizado normalmente.
        assert!(matches!(&toks[1], Token::Tag { name, .. } if name == "p"));
        // </style> case-insensitive e sem fechar (vai até o fim).
        let t2 = tokenize("<STYLE>x{}</STYLE>");
        assert!(matches!(&t2[0], Token::RawElement { content, .. } if content == "x{}"));
        let t3 = tokenize("<style>sem fechar");
        assert!(matches!(&t3[0], Token::RawElement { content, .. } if content == "sem fechar"));
    }

    #[test]
    fn raw_element_preserva_atributos() {
        // `<script src>`/`<style media>`: os atributos da abertura são preservados
        // (necessário para resolver <script src>/<link>). O `content` é o miolo cru.
        let toks = tokenize(r#"<script src="./a.js" defer>code()</script>"#);
        match &toks[0] {
            Token::RawElement { tag, attrs, content } => {
                assert_eq!(tag, "script");
                assert_eq!(attrs, r#"src="./a.js" defer"#);
                assert_eq!(content, "code()");
            }
            _ => panic!("esperava RawElement"),
        }
        // sem atributos → attrs vazio.
        let t2 = tokenize("<style>x{}</style>");
        assert!(matches!(&t2[0], Token::RawElement { attrs, .. } if attrs.is_empty()));
    }

    #[test]
    fn entidade_no_fim_e_consecutivas() {
        assert_eq!(decode_entities("fim &amp;"), "fim &");
        assert_eq!(decode_entities("&lt;&lt;&gt;&gt;"), "<<>>");
    }

    #[test]
    fn doctype_e_declaracoes_sao_ignorados() {
        // `<!DOCTYPE html>` NÃO vira token: não modelamos o nó DocumentType
        // (nodeType 10, fora do escopo) e emiti-lo como Tag empilhava o
        // documento inteiro como filho de um falso `<!doctype>`. Vale para
        // qualquer caixa (case-insensitive) e qualquer declaração `<!…>`.
        let toks = tokenize("<!DOCTYPE html><p>x</p>");
        assert!(matches!(&toks[0], Token::Tag { name, close: false, .. } if name == "p"));
        let toks2 = tokenize("<!doctype html><p>x</p>");
        assert!(matches!(&toks2[0], Token::Tag { name, .. } if name == "p"));
        // com identificadores públicos entre aspas (DTD antigo) também.
        let toks3 = tokenize("<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><p>x</p>");
        assert!(matches!(&toks3[0], Token::Tag { name, .. } if name == "p"));
    }

    #[test]
    fn maior_que_dentro_de_aspas_nao_fecha_tag() {
        // `<div title="a>b">`: o `>` dentro do valor com aspas pertence ao
        // atributo — parar no primeiro `>` cru quebrava a tag e vazava `b">`
        // como texto. O scan rastreia aspas simples e duplas.
        let toks = tokenize(r#"<div title="a>b">x</div>"#);
        match &toks[0] {
            Token::Tag { name, attrs_raw, close } => {
                assert_eq!(name, "div");
                assert!(!close);
                assert_eq!(attrs_raw, r#"title="a>b""#);
            }
            _ => panic!("esperava Tag no primeiro token"),
        }
        assert!(matches!(&toks[1], Token::Text(t) if t == "x"));
        // aspas simples também protegem o `>`.
        let t2 = tokenize("<div title='a>b'>x</div>");
        assert!(matches!(&t2[0], Token::Tag { attrs_raw, .. } if attrs_raw == "title='a>b'"));
        // fora de aspas o `>` segue fechando a tag normalmente.
        let t3 = tokenize("<br>depois");
        assert!(matches!(&t3[0], Token::Tag { name, .. } if name == "br"));
        assert!(matches!(&t3[1], Token::Text(t) if t == "depois"));
    }
}
