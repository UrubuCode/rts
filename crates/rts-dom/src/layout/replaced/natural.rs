//! Tamanho natural de controlos de formulário e tamanhos inline.

use super::*;
use super::input::{input_measure, CONTROL_FONT_FAMILY};

/// Devolve o CONTEÚDO que faz `intrinsic_outer_width`/`intrinsic_content_width`
/// (`layout/medida.rs`, que somam o frame de `css` por cima) produzir
/// exatamente `outer_w`×`outer_h` — em vez de reconstruir padding/borda à mão
/// para cada tipo de controlo, subtrai o mesmo frame que o chamador vai somar
/// depois, e os dois cancelam. `avail_w = INFINITY` porque isto é sempre uma
/// pergunta de max-content (a mesma razão de `replaced_inline_size`).
fn content_to_outer(
    css: &ComputedStyle,
    font: f32,
    ctx: &LayoutCtx,
    outer_w: f32,
    outer_h: f32,
) -> (f32, f32) {
    let resolve = ResolveCtx {
        parent_content_w: f32::INFINITY,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let border = css.border_width.unwrap_or(0.0).max(0.0);
    let pad_h = css.padding.left.resolve(&resolve).unwrap_or(0.0)
        + css.padding.right.resolve(&resolve).unwrap_or(0.0);
    let pad_v = css.padding.top.resolve(&resolve).unwrap_or(0.0)
        + css.padding.bottom.resolve(&resolve).unwrap_or(0.0);
    (
        (outer_w - pad_h - 2.0 * border).max(0.0),
        (outer_h - pad_v - 2.0 * border).max(0.0),
    )
}

/// Chamada por `intrinsic_content_width` (`layout/medida.rs`, que só
/// invoca — a explicação fica aqui e não lá porque esse ficheiro já está
/// acima do teto de 500 linhas): sem esta função, um controlo sem filhos
/// caía no ramo de bloco vazio dessa função e media 0.
///
/// Tamanho NATURAL (max-content) de um controlo de formulário sem `width`/
/// `height` — a mesma pergunta que decide a *flex base size* de
/// `flex-basis:auto` (Flexbox §9.2) e o shrink-to-fit de um bloco sem
/// `width`. `None` para o `<button>` COM filhos de texto (`<button>Clica
/// </button>`): esse já tem resposta certa pelo ramo de TEXTO genérico de
/// `intrinsic_content_width` (o texto é um filho de verdade), e responder
/// aqui também seria um segundo cálculo da mesma largura.
///
/// `input[submit/button/reset]` e `<select>`/`<textarea>` não têm essa saída:
/// o primeiro só tem o atributo `value`, sem filho nenhum; os outros dois
/// têm caixa própria mesmo vazios. Medido em
/// `tests/css/claude-controlos-tamanho-natural` (Edge/Blink): o achado que
/// origina este lote (`flex-vertical-align-effect`, WPT) era exactamente
/// este vazio — um `<input>` sem `width` caía no ramo de bloco-por-filhos
/// vazio e media 0.
pub(in crate::layout) fn control_natural_size(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    ctx: &LayoutCtx,
) -> Option<(f32, f32)> {
    let crate::dom::NodeKind::Element { tag } = &dom.node(id).kind else {
        return None;
    };
    match tag.as_str() {
        "input" => {
            let kind = dom.node(id).attr("type").map(|t| t.to_ascii_lowercase());
            if matches!(
                kind.as_deref(),
                Some("submit") | Some("button") | Some("reset")
            ) {
                let label = dom.node(id).attr("value").unwrap_or("").to_string();
                let bf = font_px(css, DEFAULT_FONT_SIZE - 3.0);
                let tw = ctx.measurer.text_width_family(
                    &label,
                    bf,
                    CONTROL_FONT_FAMILY,
                    false,
                    false,
                    false,
                );
                let lh = ctx.measurer.line_height_family(bf, CONTROL_FONT_FAMILY);
                Some(content_to_outer(css, bf, ctx, tw + 16.0, lh + 10.0))
            } else {
                Some(input_measure(dom, id, css, f32::INFINITY, None, None, None, ctx).content())
            }
        }
        "textarea" => {
            Some(input_measure(dom, id, css, f32::INFINITY, None, None, None, ctx).content())
        }
        "select" => Some(super::select::natural_content(css, ctx)),
        _ => None,
    }
}

/// Tamanho OUTER de um widget inline (`<input>`): o MESMO cálculo que a emissão
/// usa (layout_button / layout_input), para o wrap reservar a largura exata.
pub(in crate::layout) fn inline_widget_size(
    dom: &Dom,
    id: NodeIdx,
    itype: &str,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if matches!(itype, "submit" | "button" | "reset") {
        let font = font_px(&css, DEFAULT_FONT_SIZE - 3.0);
        let label = dom.node(id).attr("value").unwrap_or("").to_string();
        let tw =
            ctx.measurer
                .text_width_family(&label, font, CONTROL_FONT_FAMILY, false, false, false);
        let lh = ctx.measurer.line_height_family(font, CONTROL_FONT_FAMILY);
        return (tw + 24.0 + 6.0, lh + 10.0 + 4.0); // espelha layout_button
    }
    input_measure(dom, id, &css, avail_w, None, None, None, ctx).outer()
}
