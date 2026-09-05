//! As propriedades LÓGICAS que faltavam: `inset*` e as bordas `-inline-`/`-block-`.
//!
//! O `parse` já traduzia `padding-inline-start` e `margin-inline-start` para o
//! lado físico; `inset-inline-start` e `border-inline-start-color` caíam no
//! contador de ignoradas. Não é uma cauda: numa varredura das folhas reais o
//! WhatsApp Web escreve `border-inline-start-color` 522 vezes e
//! `inset-inline-start` 216 — o CSS moderno gerado por ferramenta já não escreve
//! `left`.
//!
//! ## Uma tradução de NOME, e não um segundo modelo de bordas
//!
//! Todo o trabalho aqui é mapear o eixo lógico no lado físico e reentregar o
//! nome traduzido a quem já sabe aplicá-lo (`style::borders`, os campos `inset_*`).
//! A alternativa — campos lógicos próprios em `ComputedStyle`, resolvidos no
//! layout — daria `direction: rtl` de graça, e foi recusada porque duplicaria o
//! modelo de bordas inteiro (doze campos) por uma propriedade de sentido.
//!
//! ## Os dois eixos lógicos seguem `writing-mode` + `direction` de verdade
//!
//! Lote `flex-writing-mode`: `inline-start`/`inline-end`/`block-start`/
//! `block-end` já não são sinónimos fixos de esquerda/topo — `to_physical`
//! pergunta a `style::text::eixo_x_forward`/`eixo_y_forward` (a MESMA
//! resposta que `layout::eixos_flex` usa para trocar o eixo do FLEX, lote
//! `flex-writing-mode`) qual physical side cada eixo lógico usa. Faltava
//! desde que `layout/flex.rs` passou a inverter o eixo principal de uma
//! `row` em `direction:rtl`/`writing-mode` vertical: sem isto,
//! `margin-inline-start` de um `gap-*-{rtl,lr,rl}` do WPT continuava a virar
//! `margin-left` sempre, e a referência (que usa a propriedade lógica para
//! simular o `gap`) passou a divergir do motor assim que o motor deixou de
//! fingir que RTL/vertical não existem. `inline-size`/`block-size` (e os
//! `min-`/`max-`) seguem a MESMA troca: `inline-size` é `width` só quando o
//! `writing-mode` é horizontal, `height` quando é vertical.

use super::lengths::{parse_inset, split_top_ws};
use super::props::ComputedStyle;
use super::text::{Direction, WritingMode, eixo_x_forward, eixo_y_forward};
use super::values::Dimension;

/// Traduz o eixo lógico de um nome de propriedade para o lado físico, sob
/// `(wm, dir)`. `"inset-inline-start"` → o lado do eixo INLINE (`left`/
/// `right` em escrita horizontal, `top`/`bottom` em vertical) que
/// `eixo_x_forward`/`eixo_y_forward` disserem ser o início; `block-start`
/// segue o eixo OPOSTO. `None` quando o nome não tem eixo lógico nenhum.
fn to_physical(prop: &str, wm: WritingMode, dir: Direction) -> Option<String> {
    let (inline_par, inline_fwd): ((&str, &str), bool) = if wm.is_horizontal() {
        (("left", "right"), eixo_x_forward(wm, dir))
    } else {
        (("top", "bottom"), eixo_y_forward(wm, dir))
    };
    let (block_par, block_fwd): ((&str, &str), bool) = if wm.is_horizontal() {
        (("top", "bottom"), eixo_y_forward(wm, dir))
    } else {
        (("left", "right"), eixo_x_forward(wm, dir))
    };
    let (inicio_inline, fim_inline) = if inline_fwd { inline_par } else { (inline_par.1, inline_par.0) };
    let (inicio_block, fim_block) = if block_fwd { block_par } else { (block_par.1, block_par.0) };
    for (logico, fisico) in [
        ("inline-start", inicio_inline),
        ("inline-end", fim_inline),
        ("block-start", inicio_block),
        ("block-end", fim_block),
    ] {
        if let Some(i) = prop.find(logico) {
            let mut out = String::with_capacity(prop.len());
            out.push_str(&prop[..i]);
            out.push_str(fisico);
            out.push_str(&prop[i + logico.len()..]);
            return Some(out);
        }
    }
    None
}

/// Escreve um dos quatro offsets pelo NOME físico do lado.
fn set_inset(css: &mut ComputedStyle, side: &str, v: Option<Dimension>) {
    match side {
        "top" => css.inset_top = v,
        "right" => css.inset_right = v,
        "bottom" => css.inset_bottom = v,
        "left" => css.inset_left = v,
        _ => {}
    }
}

/// Tenta aplicar `prop` como propriedade lógica (ou como o shorthand `inset`).
/// Devolve `false` se o nome não é uma delas — o `parse` usa isso para decidir se
/// conta a declaração como ignorada.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // `inset: <1 a 4 valores>` — a mesma ordem de qualquer shorthand de caixa
    // (top right bottom left, com os omitidos a copiar o lado oposto). Não reusa
    // `parse_edges` porque os offsets aceitam NEGATIVO e `auto`, que a caixa de
    // margem/padding trata de outra maneira.
    if prop == "inset" {
        let toks = split_top_ws(val);
        let g = |i: usize| parse_inset(&toks[i]);
        let (t, r, b, l) = match toks.len() {
            1 => (g(0), g(0), g(0), g(0)),
            2 => (g(0), g(1), g(0), g(1)),
            3 => (g(0), g(1), g(2), g(1)),
            4 => (g(0), g(1), g(2), g(3)),
            _ => return true, // valor malformado: reconhecido, sem efeito
        };
        css.inset_top = t;
        css.inset_right = r;
        css.inset_bottom = b;
        css.inset_left = l;
        return true;
    }
    // `inset-inline` / `inset-block` — os dois lados de um eixo de uma vez.
    if let Some(eixo) = prop.strip_prefix("inset-") {
        if eixo == "inline" || eixo == "block" {
            let toks = split_top_ws(val);
            if toks.is_empty() {
                return true;
            }
            let a = parse_inset(&toks[0]);
            let b = if toks.len() > 1 {
                parse_inset(&toks[1])
            } else {
                a
            };
            // mesma troca de `to_physical`: o eixo INLINE é X (left/right)
            // em escrita horizontal, Y (top/bottom) em vertical — e vice-versa
            // para o de BLOCO; o sentido vem de `eixo_x_forward`/`eixo_y_forward`.
            let wm = css.writing_mode.unwrap_or_default();
            let dir = css.direction.unwrap_or_default();
            let e_x = (eixo == "inline") == wm.is_horizontal();
            let forward = if e_x { eixo_x_forward(wm, dir) } else { eixo_y_forward(wm, dir) };
            let (inicio, fim) = if forward { (a, b) } else { (b, a) };
            if e_x {
                css.inset_left = inicio;
                css.inset_right = fim;
            } else {
                css.inset_top = inicio;
                css.inset_bottom = fim;
            }
            return true;
        }
    }

    // As DIMENSÕES lógicas: `inline-size` é a largura e `block-size` a altura,
    // em escrita horizontal — o mesmo corte LTR-horizontal que o resto do módulo
    // assume e que o cabeçalho diz por extenso.
    //
    // Reentrega ao `parse` com o nome FÍSICO em vez de escrever o campo aqui: a
    // largura tem keywords, percentagens e `calc()` que aquele braço já sabe
    // ler, e uma segunda leitura divergia dele à primeira correção.
    // Os nomes ANTIGOS do WebKit para a caixa lógica: `-webkit-margin-end` é o
    // que hoje se chama `margin-inline-end`. Chegam aqui já sem o prefixo (o
    // `parse` corta-o na última tentativa), e sem esta linha `margin-end` não
    // tem eixo lógico nenhum para traduzir e cai como desconhecida.
    let antigo = match prop {
        "margin-start" => Some("margin-inline-start"),
        "margin-end" => Some("margin-inline-end"),
        "padding-start" => Some("padding-inline-start"),
        "padding-end" => Some("padding-inline-end"),
        "border-start" => Some("border-inline-start"),
        "border-end" => Some("border-inline-end"),
        _ => None,
    };
    if let Some(moderno) = antigo {
        return try_apply(css, moderno, val);
    }

    // `inline-size` é `width` em escrita horizontal, `height` em vertical
    // (e vice-versa para `block-size`) — mesma troca de `to_physical`, mas
    // sem `direction`: uma DIMENSÃO não tem lado a inverter, só eixo.
    let wm = css.writing_mode.unwrap_or_default();
    let (largura, altura) = if wm.is_horizontal() { ("width", "height") } else { ("height", "width") };
    let dimensao = match prop {
        "inline-size" => Some(largura),
        "block-size" => Some(altura),
        "min-inline-size" => Some(if largura == "width" { "min-width" } else { "min-height" }),
        "min-block-size" => Some(if altura == "height" { "min-height" } else { "min-width" }),
        "max-inline-size" => Some(if largura == "width" { "max-width" } else { "max-height" }),
        "max-block-size" => Some(if altura == "height" { "max-height" } else { "max-width" }),
        _ => None,
    };
    if let Some(fisico) = dimensao {
        return super::parse::aplica_declaracao(css, fisico, val);
    }

    let Some(fisico) = to_physical(prop, wm, css.direction.unwrap_or_default()) else {
        return false;
    };

    // O resto da CAIXA lógica, pela mesma reentrega. O `parse` tinha
    // `padding-inline-start/end` e `margin-block-start/end` por literal mas não
    // as outras metades das mesmas famílias: `padding-block-end` caía como
    // desconhecida ao lado de uma `margin-block-end` que funcionava. Traduzir o
    // eixo e reentregar fecha as quatro famílias sem um braço por nome — e sem
    // a assimetria poder voltar.
    if fisico.starts_with("padding-") || fisico.starts_with("margin-") {
        return super::parse::aplica_declaracao(css, &fisico, val);
    }

    // `inset-inline-start` → o offset do lado físico.
    if let Some(side) = fisico.strip_prefix("inset-") {
        set_inset(css, side, parse_inset(val));
        return true;
    }
    // As bordas lógicas: o nome traduzido é EXATAMENTE o que `style::borders` já
    // reconhece, longhand (`border-left-color`) ou shorthand de lado
    // (`border-left`). Reentregar em vez de reimplementar é o ponto do módulo.
    if let Some(resto) = fisico.strip_prefix("border-") {
        if super::borders::is_longhand(&fisico) {
            super::borders::apply_longhand(css, &fisico, val);
            return true;
        }
        if let Some(side) = super::SideName::parse(resto) {
            super::borders::apply_side_shorthand(css, side, val);
            return true;
        }
    }
    false
}
