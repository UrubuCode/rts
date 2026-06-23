//! Parser HTML MÍNIMO à mão (sem crate externa) → `Vec<WidgetCmd>`.
//!
//! Objetivo do P1: renderizar HTML básico estático numa janela egui. NÃO é um
//! parser conforme à spec — é um tokenizador simples (`<tag>`, `</tag>`, texto)
//! que cobre o subconjunto necessário:
//!
//! - BLOCK: `h1`/`h2`/`h3` → `Heading{level}`; `p`/`div` → `ParagraphBegin` …
//!   `ParagraphEnd` em volta dos inlines filhos.
//! - INLINE: `b`/`strong` liga `bold`; `i`/`em` liga `italic`; texto solto vira
//!   `InlineText` com o estilo corrente.
//!
//! Robustez: tag desconhecida é IGNORADA (sem quebrar), o texto ao redor é
//! preservado. Entidades básicas `&amp;`/`&lt;`/`&gt;` são decodificadas.
//!
//! A distinção inline×block é o coração: um cabeçalho é um comando único de
//! bloco; um parágrafo abre um escopo (ParagraphBegin/End) e DENTRO dele os
//! pedaços de texto saem como `InlineText` (que a drenagem coloca lado a lado
//! com wrap). Texto fora de qualquer `<p>` também vira `InlineText` direto.

use crate::ctx::WidgetCmd;

/// Estilo inline corrente (pilha de `<b>`/`<i>`). Cada abertura empurra um nível.
#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
}

/// Um token cru do HTML: ou uma tag (com flag de fechamento) ou texto literal.
enum Token {
    /// `<nome ...>` (atributos descartados) — `close=true` para `</nome>`.
    Tag { name: String, close: bool },
    /// Texto entre tags, já com entidades decodificadas.
    Text(String),
}

/// Tokeniza o HTML char a char. Ao ver `<`, lê até `>`; senão acumula texto.
fn tokenize(html: &str) -> Vec<Token> {
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

/// Decodifica as 3 entidades básicas do P1. Ordem importa: `&amp;` por último
/// não causaria dupla-decodificação aqui pois substituímos as três de uma vez.
fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Parseia o HTML para uma fila de `WidgetCmd`, pronta para `c.cmds.extend(...)`.
///
/// Inline×block: cabeçalhos viram um `Heading` único (texto direto); `<p>`/`<div>`
/// emitem `ParagraphBegin`/`ParagraphEnd` em volta dos `InlineText`; `<b>`/`<i>`
/// só alteram o estilo corrente (não emitem comando). Texto solto vira inline.
pub fn parse_html_to_cmds(html: &str) -> Vec<WidgetCmd> {
    let tokens = tokenize(html);
    let mut cmds = Vec::new();
    // Pilha de estilos inline (bold/italic). O topo é o estilo corrente.
    let mut styles: Vec<Style> = vec![Style::default()];
    // Nível de heading aberto (1..3) e seu texto acumulado; `None` = fora de heading.
    let mut heading: Option<(u8, String)> = None;

    for tok in tokens {
        match tok {
            Token::Tag { name, close } => match name.as_str() {
                // ── BLOCK: cabeçalhos ──────────────────────────────────────
                "h1" | "h2" | "h3" => {
                    let level = name.as_bytes()[1] - b'0';
                    if close {
                        // Fecha: emite o Heading acumulado.
                        if let Some((lvl, text)) = heading.take() {
                            cmds.push(WidgetCmd::Heading { level: lvl, text });
                        }
                    } else {
                        heading = Some((level, String::new()));
                    }
                }
                // ── BLOCK: parágrafo / div ─────────────────────────────────
                "p" | "div" => {
                    if close {
                        cmds.push(WidgetCmd::ParagraphEnd);
                    } else {
                        cmds.push(WidgetCmd::ParagraphBegin);
                    }
                }
                // ── INLINE: negrito ────────────────────────────────────────
                "b" | "strong" => set_inline(&mut styles, close, |st| st.bold = true),
                // ── INLINE: itálico ────────────────────────────────────────
                "i" | "em" => set_inline(&mut styles, close, |st| st.italic = true),
                // Tag desconhecida: ignorada (texto ao redor é preservado).
                _ => {}
            },
            Token::Text(text) => {
                if text.trim().is_empty() {
                    continue; // whitespace puro entre tags — descarta.
                }
                if let Some((_, acc)) = heading.as_mut() {
                    // Dentro de um heading: acumula o texto (sem inline mix).
                    acc.push_str(&text);
                } else {
                    let st = *styles.last().unwrap();
                    cmds.push(WidgetCmd::InlineText {
                        text,
                        bold: st.bold,
                        italic: st.italic,
                    });
                }
            }
        }
    }
    // Heading não fechado (HTML malformado): emite o que houver, defensivo.
    if let Some((level, text)) = heading.take() {
        cmds.push(WidgetCmd::Heading { level, text });
    }
    cmds
}

/// Aplica abertura/fechamento de uma tag inline na pilha de estilos: ao abrir,
/// empurra uma cópia do estilo corrente com `apply` aplicado; ao fechar, volta.
fn set_inline(styles: &mut Vec<Style>, close: bool, apply: impl FnOnce(&mut Style)) {
    if close {
        if styles.len() > 1 {
            styles.pop();
        }
    } else {
        let mut next = *styles.last().unwrap();
        apply(&mut next);
        styles.push(next);
    }
}
