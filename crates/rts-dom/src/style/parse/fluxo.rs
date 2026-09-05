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
        "flex-basis" => set_if(&mut css.flex_basis, parse_flex_basis(val)),
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
        // `parse_dimension_min_max`, não `parse_dimension`, nos quatro: um
        // CLAMP aceita `min-content` — no eixo inline resolve para o
        // min-content REAL (`crate::table::min_content`, via
        // `layout/flex_limites.rs`); no eixo de bloco é o piso DECLARADO de
        // um item flex (encolhimento de coluna, `layout/coluna_shrink.rs` —
        // não pode desaparecer só porque o automático some sob overflow
        // não-visível). Ver `Dimension::MinContent`.
        "min-width" => set_if(&mut css.min_width, parse_dimension_min_max(val)),
        "max-width" => set_if(&mut css.max_width, parse_dimension_min_max(val)),
        "min-height" => set_if(&mut css.min_height, parse_dimension_min_max(val)),
        "max-height" => set_if(&mut css.max_height, parse_dimension_min_max(val)),
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
        "writing-mode" => set_if(&mut css.writing_mode, crate::style::WritingMode::parse(val)),
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

/// `flex-basis`, com a keyword `content` (Flexbox §7.2) além do que
/// `parse_dimension` já cobre: sizing sempre pelo conteúdo, tratado como
/// `max-content` (o único intrínseco que este motor mede) — sem isto
/// `content` caía no `None` de `parse_dimension` e, no shorthand `flex: 0 0
/// content`, o `unwrap_or(Percent(0.0))` de baixo lia-o como 0% (achado ao
/// medir `flexbox-flex-basis-content-004a`: um item `flex-shrink:0` colapsava
/// em vez de manter a altura do conteúdo).
fn parse_flex_basis(v: &str) -> Option<Dimension> {
    if v.trim().eq_ignore_ascii_case("content") {
        return Some(Dimension::MaxContent);
    }
    parse_dimension(v)
}


/// Aplica o shorthand `flex: none | auto | <grow> [<shrink>] [<basis>]`.
/// Mapeamentos da spec: `none` = 0 0 auto; `auto` = 1 1 auto; UM número =
/// grow=N shrink=1 basis=0% (o `.col { flex: 1 0 0% }` já vem com os três).
///
/// Movida de `mod.rs` (no teto de linhas, "não cresce" — `PLAN.md` §1) para
/// junto do seu único chamador, com a correcção que faltava: CSS Values §6.1
/// só dispensa unidade do número ZERO — `flex: 0 0 4` tem um 3º token "4"
/// que NÃO é um `<length>` válido, e um shorthand `flex` com QUALQUER parte
/// inválida cai INTEIRO (grow/shrink/basis ficam nos iniciais 0/1/auto,
/// nunca escritos por esta função). A versão anterior lia "4" como `4px` via
/// `parse_dimension`, que aceita número puro como px para as propriedades
/// que o admitem — correcto ali, errado aqui, onde a spec não admite.
fn apply_flex_shorthand(css: &mut ComputedStyle, val: &str) {
    let v = val.trim();
    if v.eq_ignore_ascii_case("none") {
        set_if(&mut css.flex_grow, Some(0.0));
        set_if(&mut css.flex_shrink, Some(0.0));
        set_if(&mut css.flex_basis, Some(Dimension::Auto));
        return;
    }
    if v.eq_ignore_ascii_case("auto") {
        set_if(&mut css.flex_grow, Some(1.0));
        set_if(&mut css.flex_shrink, Some(1.0));
        set_if(&mut css.flex_basis, Some(Dimension::Auto));
        return;
    }
    let toks: Vec<&str> = v.split_whitespace().collect();
    // separa os NÚMEROS iniciais (grow [shrink]) de uma dimensão final (basis).
    let mut nums: Vec<f32> = Vec::new();
    let mut basis: Option<Dimension> = None;
    for t in &toks {
        if basis.is_none() && nums.len() < 2 {
            if let Ok(n) = t.parse::<f32>() {
                nums.push(n.max(0.0));
                continue;
            }
        }
        if basis.is_none() {
            // um número BRUTO (sem sufixo de unidade) só é válido como basis
            // quando é exactamente zero — qualquer outro invalida o
            // shorthand INTEIRO (nada é escrito, nem os tokens já lidos).
            if t.parse::<f32>().map(|n| n != 0.0).unwrap_or(false) {
                return;
            }
            basis = parse_flex_basis(t);
        }
    }
    match (nums.len(), basis) {
        // `flex: 200px` — só a basis.
        (0, Some(b)) => set_if(&mut css.flex_basis, Some(b)),
        (0, None) => {} // inválido: ignora (robustez)
        (n, b) => {
            set_if(&mut css.flex_grow, Some(nums[0]));
            set_if(&mut css.flex_shrink, Some(if n >= 2 { nums[1] } else { 1.0 }));
            // UM número sem basis → basis 0% (spec); com basis explícita, usa-a.
            set_if(&mut css.flex_basis, Some(b.unwrap_or(Dimension::Percent(0.0))));
        }
    }
}
