//! `padding`, `margin`, as bordas e o `outline`
//!
//! Os braços vieram do `match` de `aplica_declaracao` VERBATIM — a forma
//! `try_apply` é a mesma que os seis módulos vizinhos já usam, e a
//! indentação é a mesma nos dois sítios.

use super::*;

/// O lado FÍSICO de `inline-start`/`inline-end`/`block-start`/`block-end`
/// sob `writing-mode`+`direction` — a mesma pergunta que `style::logical::
/// to_physical` resolve para as OUTRAS logicas (`padding-inline`, `inset-*`),
/// repetida aqui porque estes três nomes (Tailwind/WhatsApp Web os escrevem
/// por milhares) já tinham braço PRÓPRIO no `match` antes de `logical.rs`
/// existir — RULE 0b: mover para lá duplicaria o `match` de `try_apply`
/// inteiro por três nomes. `is_inline`: eixo INLINE (`true`) ou de BLOCO
/// (`false`); `is_start`: `-start` (`true`) ou `-end` (`false`).
fn lado_logico(is_inline: bool, is_start: bool, css: &ComputedStyle) -> crate::style::SideName {
    use crate::style::SideName as S;
    let wm = css.writing_mode.unwrap_or_default();
    let dir = css.direction.unwrap_or_default();
    let e_x = is_inline == wm.is_horizontal();
    let forward = if e_x {
        crate::style::text::eixo_x_forward(wm, dir)
    } else {
        crate::style::text::eixo_y_forward(wm, dir)
    };
    match (e_x, forward == is_start) {
        (true, true) => S::Left,
        (true, false) => S::Right,
        (false, true) => S::Top,
        (false, false) => S::Bottom,
    }
}

pub(in crate::style::parse) fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "padding" => set_edges(&mut css.padding, parse_edges(val, Caixa::Padding)),
        "padding-top" => set_side(&mut css.padding.top, parse_side(val, Caixa::Padding)),
        "padding-right" => set_side(&mut css.padding.right, parse_side(val, Caixa::Padding)),
        "padding-bottom" => set_side(&mut css.padding.bottom, parse_side(val, Caixa::Padding)),
        "padding-left" => set_side(&mut css.padding.left, parse_side(val, Caixa::Padding)),
        // Props LÓGICAS (Tailwind v4 usa `px-N`→padding-inline, `py-N`→padding-block
        // em TUDO): inline = left+right, block = top+bottom (modo horizontal LTR).
        "padding-inline" => {
            let s = parse_side(val, Caixa::Padding);
            css.padding.left = s;
            css.padding.right = s;
        }
        "padding-block" => {
            let s = parse_side(val, Caixa::Padding);
            css.padding.top = s;
            css.padding.bottom = s;
        }
        "padding-inline-start" | "padding-inline-end" => {
            // `writing-mode`/`direction` decidem o lado (`lado_logico`, lote
            // `flex-writing-mode`) — sem isto, `flex.rs` trocando o eixo
            // principal em `rtl`/vertical divergia de uma folha que já usava
            // a lógica.
            let s = parse_side(val, Caixa::Padding);
            match lado_logico(true, prop == "padding-inline-start", css) {
                crate::style::SideName::Left => css.padding.left = s,
                crate::style::SideName::Right => css.padding.right = s,
                crate::style::SideName::Top => css.padding.top = s,
                crate::style::SideName::Bottom => css.padding.bottom = s,
            }
        }
        // margin aceita `auto` (centralização); padding não.
        "margin" => set_edges(&mut css.margin, parse_edges(val, Caixa::Margem)),
        "margin-top" => set_side(&mut css.margin.top, parse_side(val, Caixa::Margem)),
        "margin-right" => set_side(&mut css.margin.right, parse_side(val, Caixa::Margem)),
        "margin-bottom" => set_side(&mut css.margin.bottom, parse_side(val, Caixa::Margem)),
        "margin-left" => set_side(&mut css.margin.left, parse_side(val, Caixa::Margem)),
        "margin-inline" => {
            let s = parse_side(val, Caixa::Margem);
            css.margin.left = s;
            css.margin.right = s;
        }
        "margin-block" => {
            let s = parse_side(val, Caixa::Margem);
            css.margin.top = s;
            css.margin.bottom = s;
        }
        // `writing-mode`/`direction` decidem o lado — mesma regra de
        // `padding-inline-start`/`-end` acima (`lado_logico`).
        "margin-inline-start" | "margin-inline-end" => {
            let s = parse_side(val, Caixa::Margem);
            match lado_logico(true, prop == "margin-inline-start", css) {
                crate::style::SideName::Left => css.margin.left = s,
                crate::style::SideName::Right => css.margin.right = s,
                crate::style::SideName::Top => css.margin.top = s,
                crate::style::SideName::Bottom => css.margin.bottom = s,
            }
        }
        "margin-block-start" | "margin-block-end" => {
            // eixo de BLOCO: `false` no primeiro argumento — em `writing-mode`
            // vertical isto é X (left/right), não top/bottom sempre.
            let s = parse_side(val, Caixa::Margem);
            match lado_logico(false, prop == "margin-block-start", css) {
                crate::style::SideName::Left => css.margin.left = s,
                crate::style::SideName::Right => css.margin.right = s,
                crate::style::SideName::Top => css.margin.top = s,
                crate::style::SideName::Bottom => css.margin.bottom = s,
            }
        }
        // shorthand `border: <width> <style> <color>` (qualquer ordem, qualquer
        // omitível). Setar os 3 de uma vez. (Por-lado fica para fase 2.)
        "border" => apply_border_shorthand(css, val),
        "border-width" => crate::style::borders::apply_width_shorthand(css, val),
        "border-style" => crate::style::borders::apply_style_shorthand(css, val),
        "border-color" => crate::style::borders::apply_color_shorthand(css, val),
        // O campo UNICO continua a responder o que sempre respondeu (quem o
        // le nao pode mudar de resposta por causa dos cantos); os quatro
        // cantos sao escritos por cima, sem lhe tocar. Ver `style::radius`.
        "border-radius" => {
            set_if(&mut css.corner_radius, parse_len(val));
            crate::style::radius::apply_shorthand(css, val);
        }
        // ── Bordas POR LADO: `border-top: 1px solid #ccc` e as 12 longhands.
        // Uma barra separadora é quase sempre um lado só; pintá-la com a borda
        // uniforme daria uma moldura fechada (ver `style::borders`).
        "border-top" | "border-right" | "border-bottom" | "border-left" => {
            if let Some(side) = crate::style::SideName::parse(&prop["border-".len()..]) {
                crate::style::borders::apply_side_shorthand(css, side, val);
            }
        }
        _ if crate::style::borders::is_longhand(&prop) => {
            crate::style::borders::apply_longhand(css, &prop, val)
        }
        // `outline`: uma borda que não ocupa espaço (fora do box model).
        "outline" => crate::style::borders::apply_outline_shorthand(css, val),
        "outline-width" => set_if(&mut css.outline_width, crate::style::borders::parse_width_token(val)),
        "outline-style" => {
            css.outline_style = if val.trim().eq_ignore_ascii_case("auto") {
                Some(BorderStyle::Solid)
            } else {
                BorderStyle::parse(val)
            }
        }
        "outline-color" => set_if(&mut css.outline_color, parse_color(val)),
        "outline-offset" => set_if(&mut css.outline_offset, parse_signed_px(val)),
        _ => return false,
    }
    true
}
