//! Tamanho, `display`, flex, posição e a cauda de texto e listas
//!
//! Os braços vieram do `match` de `aplica_declaracao` VERBATIM — a forma
//! `try_apply` é a mesma que os seis módulos vizinhos já usam, e a
//! indentação é a mesma nos dois sítios.

use super::*;

pub(in crate::style::parse) fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "width" => set_if(&mut css.width, parse_dimension(val)),
        // `box-sizing: border-box | content-box` — border-box faz o `width`
        // incluir padding+border (3 cards de 32% cabem). Default content-box.
        "box-sizing" => set_if(&mut css.border_box, Some(val.eq_ignore_ascii_case("border-box"))),
        // `display` — o eixo/fluxo dos filhos, do CSS (não mais só do defineBlock).
        "display" => {
            set_if(&mut css.display, parse_display(val));
            // A distinção que o `DisplayKind` não carrega: `flow-root` é uma
            // caixa de bloco que ESTABELECE um contexto de formatação, e essa é
            // a única coisa que a separa de `block`. O parse aceitava a palavra
            // e deitava fora o significado dela.
            set_if(
                &mut css.flow_root,
                val.trim().eq_ignore_ascii_case("flow-root").then_some(true),
            );
        }
        // `flex-wrap` — combina com display:flex para promover a FlexWrap.
        // `nowrap`/`wrap`/`wrap-reverse`: os três estados de `FlexWrap`.
        "flex-wrap" => set_if(&mut css.flex_wrap, FlexWrap::parse(val)),
        // ── Flexbox: alinhamento + gap + direção ──────────────────────────────
        "justify-content" => set_if(&mut css.justify, JustifyContent::parse(val)),
        "align-items" => set_if(&mut css.align_items, AlignItems::parse(val)),
        "align-self" => {
            // `auto` = herda o align-items do container (campo fica None).
            css.align_self = if val.eq_ignore_ascii_case("auto") {
                None
            } else {
                AlignItems::parse(val)
            }
        }
        "order" => css.order = val.trim().parse::<i32>().ok(),
        "flex-grow" => css.flex_grow = val.trim().parse::<f32>().ok().filter(|v| *v >= 0.0),
        "flex-shrink" => css.flex_shrink = val.trim().parse::<f32>().ok().filter(|v| *v >= 0.0),
        "flex-basis" => set_if(&mut css.flex_basis, parse_dimension(val)),
        // shorthand `flex`: none | auto | <grow> [<shrink>] [<basis>] — o
        // `.col` do Bootstrap é `flex: 1 0 0%`.
        "flex" => apply_flex_shorthand(css, val),
        "flex-direction" => set_if(&mut css.flex_direction, FlexDirection::parse(val)),
        "column-gap" => set_if(&mut css.gap, parse_dimension(val)),
        "row-gap" => set_if(&mut css.row_gap, parse_dimension(val)),
        // `gap: <row> <col>` (1 valor = ambos; 2 = row col).
        "gap" => {
            let (rg, cg) = parse_gap_pair(val);
            css.row_gap = rg;
            css.gap = cg;
        }
        "height" => set_if(&mut css.height, parse_dimension(val)),
        "min-width" => set_if(&mut css.min_width, parse_dimension(val)),
        "max-width" => set_if(&mut css.max_width, parse_dimension(val)),
        "min-height" => set_if(&mut css.min_height, parse_dimension(val)),
        "max-height" => set_if(&mut css.max_height, parse_dimension(val)),
        // `position` + offsets (top/right/bottom/left). Os offsets aceitam
        // negativos (deslocam para fora) — parse_dimension rejeita <0, então
        // px negativo entra por parse direto.
        "float" => set_if(&mut css.float_side, FloatSide::parse(val)),
        "position" => set_if(&mut css.position, Position::parse(val)),
        "z-index" => css.z_index = val.trim().parse::<i32>().ok(),
        "top" => set_if(&mut css.inset_top, parse_inset(val)),
        "right" => set_if(&mut css.inset_right, parse_inset(val)),
        "bottom" => set_if(&mut css.inset_bottom, parse_inset(val)),
        "left" => set_if(&mut css.inset_left, parse_inset(val)),
        // ── Texto / listas / fluxo (ver `style::text` p/ o que cada uma faz) ──
        "vertical-align" => set_if(&mut css.vertical_align, crate::style::VerticalAlign::parse(val)),
        "clear" => set_if(&mut css.clear, crate::style::Clear::parse(val)),
        "word-break" => set_if(&mut css.word_break, crate::style::WordBreak::parse(val)),
        // `word-wrap` é o nome legado de `overflow-wrap` (MDN: alias).
        "overflow-wrap" | "word-wrap" => set_if(&mut css.overflow_wrap, crate::style::OverflowWrap::parse(val)),
        "direction" => set_if(&mut css.direction, crate::style::Direction::parse(val)),
        // `text-indent` aceita negativo (o truque de esconder texto atrás da
        // margem, comum em logos com fundo).
        "text-indent" => set_if(&mut css.text_indent, parse_dimension_signed(val)),
        "list-style-type" => set_if(&mut css.list_style_type, crate::style::ListStyleType::parse(val)),
        "list-style-position" => {
            css.list_style_position = crate::style::ListStylePosition::parse(val)
        }
        // ── Tabela ────────────────────────────────────────────────────────
        "border-collapse" => set_if(&mut css.border_collapse, crate::style::BorderCollapse::parse(val)),
        "border-spacing" => set_if(&mut css.border_spacing, crate::style::BorderSpacing::parse(val)),
        "table-layout" => set_if(&mut css.table_layout, crate::style::TableLayout::parse(val)),
        "list-style-image" => set_if(&mut css.list_style_image, Some(val.trim().to_string())),
        // `list-style: <type> || <position> || <image>` — os três em qualquer
        // ordem, e agora os três GUARDADOS: a posição deixou de ser descartada
        // quando o marcador passou a ser desenhado.
        //
        // A ordem dos ramos importa: `none` é um valor válido de `type` E o
        // ficheiro não tem como saber a qual dos dois o autor se referia, por
        // isso o `type` é tentado ANTES da posição — `outside`/`inside` não
        // são valores de `type`, portanto não há ambiguidade no outro sentido.
        "list-style" => {
            for tok in val.split_whitespace() {
                if tok.to_ascii_lowercase().starts_with("url(") {
                    set_if(&mut css.list_style_image, Some(tok.to_string()));
                } else if let Some(t) = crate::style::ListStyleType::parse(tok) {
                    set_if(&mut css.list_style_type, Some(t));
                } else if let Some(p) = crate::style::ListStylePosition::parse(tok) {
                    set_if(&mut css.list_style_position, Some(p));
                }
            }
        }
        // `cursor` — guardado cru; quem o usa é o backend de janela.
        "cursor" => set_if(&mut css.cursor, Some(val.trim().to_ascii_lowercase())),
        // `flex-flow: <direction> || <wrap>` (MDN) — só expande.
        "flex-flow" => {
            for tok in val.split_whitespace() {
                if let Some(d) = FlexDirection::parse(tok) {
                    set_if(&mut css.flex_direction, Some(d));
                } else if let Some(w) = FlexWrap::parse(tok) {
                    set_if(&mut css.flex_wrap, Some(w));
                }
            }
        }
        _ => return false,
    }
    true
}
