//! Decodificação de referências de caractere HTML (HTML §13.2.5.72-.80):
//! nomeadas (a tabela completa de 2231 nomes em `dom::entities`, gerada de
//! `entities.json`) e numéricas (`&#NN;`/`&#xNN;`). Módulo próprio — extraído
//! de `html.rs` quando cobrir a tabela inteira (antes um subconjunto de ~30
//! nomes à mão) e as duas exceções da spec (legadas sem `;`, tabela de
//! substituição Windows-1252) levou `html.rs` acima do teto de 500 linhas.

/// Maior nome de referência nomeada da tabela gerada (`CounterClockwiseContourIntegral`,
/// 31 chars) — limita a janela de busca do `;` de fechamento.
const MAX_NAMED_LEN: usize = 31;
/// Maior nome LEGADO (sem `;`) da tabela — `plusmn`, `uacute` etc, 6 chars.
const MAX_LEGACY_LEN: usize = 6;

/// Decodifica entidades HTML no TEXTO (contexto não-atributo): toda referência
/// nomeada reconhecida, mesmo as ~106 legadas sem `;` (HTML §13.2.5.72-.80).
/// `pub(crate)` — usada pelo tokenizador em `html.rs`.
pub(crate) fn decode_entities(s: &str) -> String {
    decode_entities_ctx(s, false)
}

/// Decodifica entidades HTML dentro de um valor de ATRIBUTO: como o texto,
/// exceto que uma referência legada (sem `;`) NÃO é decodificada quando
/// seguida de `=` ou alfanumérico — regra histórica do HTML (§13.2.5.72,
/// "was consumed as part of an attribute") para não quebrar `href="?a&copy=1"`
/// tratando `&copy` como `©`. `pub(crate)` — usada por `dom.rs` ao parsear
/// atributos.
pub(crate) fn decode_entities_attr(s: &str) -> String {
    decode_entities_ctx(s, true)
}

/// Implementação única das duas funções acima (sem `.replace` encadeado, que
/// não pega as numéricas e arrisca dupla-decodificação). `in_attribute`
/// habilita a regra de supressão das referências legadas sem `;` descrita
/// acima. Uma entidade desconhecida ou malformada é deixada literal —
/// robustez de parser real.
fn decode_entities_ctx(s: &str, in_attribute: bool) -> String {
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
        let rest = &s[i + 1..];
        // Numérica: `&#NN;` decimal ou `&#xNN;`/`&#XNN;` hex. O `;` final é
        // opcional na spec (parse error, mas ainda consumida) — aceitamos
        // ambas as formas.
        if let Some(numeric) = rest.strip_prefix('#') {
            if let Some((ch, consumed)) = decode_numeric_reference(numeric) {
                crate::bump!(entities_decoded);
                out.push(ch);
                i += 1 + 1 + consumed; // `&` + `#` + dígitos(+`;`)
                continue;
            }
            out.push('&');
            i += 1;
            continue;
        }
        // Nomeada COM `;`: tenta o corpo entre `&` e o primeiro `;` numa
        // janela curta (entidades têm no máximo `MAX_NAMED_LEN` chars).
        if let Some(rel) = rest.find(';').filter(|&rel| rel <= MAX_NAMED_LEN) {
            let body = &rest[..rel];
            if let Some(sub) = crate::dom::entities::decode_named_with_semicolon(body) {
                crate::bump!(entities_decoded);
                out.push_str(sub);
                i += 1 + rel + 1; // pula `&body;`
                continue;
            }
        }
        // Nomeada SEM `;`: só as ~106 legadas, por PREFIXO mais longo
        // possível (a spec consome "the longest sequence of characters that
        // could be interpreted as a named reference").
        let mut matched = None;
        let max_len = MAX_LEGACY_LEN.min(rest.len());
        for len in (1..=max_len).rev() {
            // `rest` pode ter menos que `len` BYTES utf8-válidos no meio de
            // um char multibyte; nomes legados são ASCII, então cortar por
            // char_indices evita partir um char ao meio.
            if !rest.is_char_boundary(len) {
                continue;
            }
            let candidate = &rest[..len];
            if let Some(sub) = crate::dom::entities::decode_named_legacy_no_semicolon(candidate)
            {
                matched = Some((len, sub));
                break;
            }
        }
        if let Some((len, sub)) = matched {
            let next = rest[len..].chars().next();
            let suppressed = in_attribute
                && matches!(next, Some(c) if c == '=' || c.is_ascii_alphanumeric());
            if !suppressed {
                crate::bump!(entities_decoded);
                out.push_str(sub);
                i += 1 + len;
                continue;
            }
        }
        // Desconhecida (ou legada suprimida em contexto de atributo): deixa
        // o `&` literal e segue.
        crate::bump!(entities_unknown);
        out.push('&');
        i += 1;
    }
    out
}

/// Decodifica o corpo de uma referência numérica (após o `#`, ANTES do `;`
/// opcional). Retorna o char decodificado e quantos bytes do corpo (dígitos +
/// `;` opcional) foram consumidos. `None` se não há dígitos válidos.
///
/// Aplica a tabela de substituição do HTML §13.5 (referências que caíam no
/// intervalo C1 de Windows-1252 por causa de encoders antigos), `0` e
/// surrogates viram U+FFFD, e qualquer coisa acima de U+10FFFF também.
fn decode_numeric_reference(body: &str) -> Option<(char, usize)> {
    let (digits_str, radix, prefix_len) = if let Some(hex) = body.strip_prefix(['x', 'X']) {
        (hex, 16, 1)
    } else {
        (body, 10, 0)
    };
    let digit_len = digits_str
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits_str.len());
    if digit_len == 0 {
        return None; // `&#;` ou `&#x;`: nenhum dígito, não é uma referência.
    }
    let code = u32::from_str_radix(&digits_str[..digit_len], radix).unwrap_or(0);
    let mut consumed = prefix_len + digit_len;
    if body[consumed..].starts_with(';') {
        consumed += 1;
    }
    Some((numeric_reference_char(code), consumed))
}

/// Aplica a tabela de substituição do HTML §13.5 "numeric character
/// reference end state" a um código numérico já parseado.
fn numeric_reference_char(code: u32) -> char {
    let mapped = match code {
        0x00 => 0xFFFD,
        0x80 => 0x20AC,
        0x82 => 0x201A,
        0x83 => 0x0192,
        0x84 => 0x201E,
        0x85 => 0x2026,
        0x86 => 0x2020,
        0x87 => 0x2021,
        0x88 => 0x02C6,
        0x89 => 0x2030,
        0x8A => 0x0160,
        0x8B => 0x2039,
        0x8C => 0x0152,
        0x8E => 0x017D,
        0x91 => 0x2018,
        0x92 => 0x2019,
        0x93 => 0x201C,
        0x94 => 0x201D,
        0x95 => 0x2022,
        0x96 => 0x2013,
        0x97 => 0x2014,
        0x98 => 0x02DC,
        0x99 => 0x2122,
        0x9A => 0x0161,
        0x9B => 0x203A,
        0x9C => 0x0153,
        0x9E => 0x017E,
        0x9F => 0x0178,
        0xD800..=0xDFFF => 0xFFFD, // surrogate: nunca um char válido isolado.
        _ if code > 0x10FFFF => 0xFFFD,
        other => other,
    };
    char::from_u32(mapped).unwrap_or('\u{FFFD}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_entities() {
        assert_eq!(decode_entities("a &lt; b &gt; c &amp; d"), "a < b > c & d");
        assert_eq!(
            decode_entities("&quot;aspas&quot; &apos;simples&apos;"),
            "\"aspas\" 'simples'"
        );
        assert_eq!(decode_entities("x&nbsp;y"), "x\u{00A0}y");
        assert_eq!(
            decode_entities("&copy; 2026 &mdash; ok&hellip;"),
            "\u{00A9} 2026 \u{2014} ok\u{2026}"
        );
    }

    #[test]
    fn numeric_entities() {
        assert_eq!(decode_entities("&#65;&#66;&#67;"), "ABC"); // decimal
        assert_eq!(decode_entities("&#x41;&#x42;"), "AB"); // hex minúsculo
        assert_eq!(decode_entities("&#X41;"), "A"); // hex maiúsculo
        assert_eq!(decode_entities("caf&#233;"), "café"); // não-ASCII decimal
        assert_eq!(decode_entities("&#9731;"), "☃"); // BMP fora do Latin-1
    }

    #[test]
    fn malformed_stay_literal() {
        assert_eq!(decode_entities("Tom & Jerry"), "Tom & Jerry"); // `&` solto
        assert_eq!(decode_entities("&naoexiste;"), "&naoexiste;"); // nome desconhecido
        assert_eq!(decode_entities("&#abc;"), "&#abc;"); // numérica inválida
        assert_eq!(decode_entities("100% & mais"), "100% & mais");
        assert_eq!(decode_entities("sem ampersand"), "sem ampersand"); // atalho sem `&`
    }

    #[test]
    fn entities_from_full_html5_set() {
        // `&NewLine;` e `&Tab;` são referências XML-históricas presentes na
        // tabela completa do HTML5 mas ausentes do subconjunto anterior de
        // ~30 nomes — a causa da issue: um `.xht` de WPT com `AB&NewLine;`
        // vazava o texto literal `&NewLine;` na pintura.
        assert_eq!(decode_entities("AB&NewLine;CD"), "AB\nCD");
        assert_eq!(decode_entities("a&Tab;b"), "a\tb");
        // referência desconhecida: verbatim.
        assert_eq!(decode_entities("&foo;"), "&foo;");
    }

    #[test]
    fn numeric_entities_special_cases() {
        assert_eq!(decode_entities("&#10;"), "\n");
        assert_eq!(decode_entities("&#x0A;"), "\n");
        // 0 vira U+FFFD (nunca o NUL literal).
        assert_eq!(decode_entities("&#0;"), "\u{FFFD}");
        // tabela de substituição Windows-1252 do HTML §13.5: 128 decimal era
        // C1 (controle) em Latin-1, mas encoders legados usavam Windows-1252
        // onde esse byte é o símbolo do euro.
        assert_eq!(decode_entities("&#128;"), "\u{20AC}");
        // surrogate isolado: também U+FFFD.
        assert_eq!(decode_entities("&#xD800;"), "\u{FFFD}");
        // sem `;` de fechamento: a spec ainda consome (parse error, não recusa).
        assert_eq!(decode_entities("&#65x"), "Ax");
    }

    #[test]
    fn legacy_entity_without_semicolon() {
        // `&amp` sem `;` é uma das ~106 legadas (HTML §13.5) e É decodificada
        // em TEXTO.
        assert_eq!(decode_entities("Tom &amp Jerry"), "Tom & Jerry");
        // Em valor de ATRIBUTO, a mesma referência legada NÃO é decodificada
        // quando seguida de `=` ou alfanumérico (regra histórica para não
        // quebrar algo como `href="?a&ampx=1"` interpretando `&amp` como `&`).
        assert_eq!(decode_entities_attr("?a&ampx=1"), "?a&ampx=1");
        assert_eq!(decode_entities_attr("?a&amp=1"), "?a&amp=1");
        // seguida de espaço (nem `=` nem alfanumérico): decodifica normalmente.
        assert_eq!(decode_entities_attr("a &amp b"), "a & b");
    }

    #[test]
    fn entity_at_end_and_consecutive() {
        assert_eq!(decode_entities("fim &amp;"), "fim &");
        assert_eq!(decode_entities("&lt;&lt;&gt;&gt;"), "<<>>");
    }
}
