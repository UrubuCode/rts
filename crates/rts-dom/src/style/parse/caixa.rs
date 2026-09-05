//! `padding`, `margin`, as bordas e o `outline`
//!
//! Os braços vieram do `match` de `aplica_declaracao` VERBATIM — a forma
//! `try_apply` é a mesma que os seis módulos vizinhos já usam, e a
//! indentação é a mesma nos dois sítios.

use super::*;

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
        // `padding-inline-start`/`-end` NÃO entram aqui: o lado físico depende
        // de `direction`, que só se conhece POR ELEMENTO — `style::logical`
        // trata as duas (cabeçalho "Quando isto resolve"), com o mesmo `parse_side`
        // reentregue por `aplica_declaracao("padding-left"/"padding-right", …)`.
        // Duplicar aqui, mesmo que fosse direction-aware, era a MESMA tradução
        // escrita duas vezes — achado pelo WPT `gap-007-rtl` (lote
        // `flex-reverse-order`): este braço claimava o nome primeiro, sempre
        // LTR, e `style::logical::try_apply` nunca chegava a ser chamado.
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
        // `margin-block-start`/`-end` NÃO entram aqui — mesmo motivo do
        // `padding-inline-*` acima, e não só o `direction`: desde o lote
        // `flex-writing-mode-2`, o lado físico do eixo de BLOCO também
        // depende de `writing-mode` (herdado, só conhecido por elemento) —
        // um braço fixo `top`/`bottom` aqui escondia `style::logical::
        // try_apply` por completo para esta família, como acontecia com
        // `margin-inline-start`/`-end` antes do lote `flex-reverse-order`
        // (achado pelo WPT `gap-002-lr`: a referência simula o `gap`
        // principal de um `flex-direction:column` com `margin-block-start`,
        // e sob `writing-mode:vertical-lr` essa margem é a ESQUERDA/DIREITA,
        // não `top`).
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
