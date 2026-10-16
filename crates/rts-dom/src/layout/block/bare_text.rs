//! Um nó de TEXTO que chega sozinho ao fluxo de blocos — o `abc` de
//! `<div style="display:table">abc</div>` (a célula anónima da grelha é o
//! próprio nó de texto, `table/grid.rs`), ou texto directo na raiz.
//!
//! `block.rs` pintava-o com a fonte por omissão (16px, altura de linha do
//! medidor) e ignorava o pai: numa página com `font: 16px/20px` a linha saía
//! com 18 e não 20, e a cor/negrito/família do pai perdiam-se. O texto solto
//! HERDA — é o pai que diz a fonte — e é isso que aqui se lê
//! (`claude-table-texto-solto-sem-celula`: a `display:table` com texto mede
//! os mesmos 20px que o bloco irmão).
//!
//! Vive à parte porque `block.rs` já passa o tecto das 500 linhas.

use super::*;

/// Pinta o texto em `(x, y)` com o estilo do pai e devolve `(largura, altura)`
/// da linha. Texto só de whitespace não cria linha nenhuma: é o espaço entre
/// dois `<tr>`, não conteúdo.
pub(super) fn layout_bare_text(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    t: &str,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    if t.trim().is_empty() {
        return (0.0, 0.0);
    }
    let parent_css = dom
        .node(id)
        .parent
        .and_then(|p| dom.computed_style_idx(p))
        .unwrap_or_default();
    let ws = parent_css.white_space.unwrap_or(crate::style::WhiteSpace::Normal);
    let text_cow: std::borrow::Cow<'_, str> = if ws.preserves_spaces() {
        std::borrow::Cow::Borrowed(t)
    } else {
        let trimmed = t.trim();
        if trimmed.chars().any(crate::inline_box::e_espaco_css) || trimmed.contains('\n') || trimmed.contains('\t') {
            let mut s = String::with_capacity(trimmed.len());
            let mut in_space = false;
            for ch in trimmed.chars() {
                if ch == '\n' || ch == '\t' || crate::inline_box::e_espaco_css(ch) {
                    if !in_space {
                        s.push(' ');
                        in_space = true;
                    }
                } else {
                    s.push(ch);
                    in_space = false;
                }
            }
            std::borrow::Cow::Owned(s)
        } else {
            std::borrow::Cow::Borrowed(trimmed)
        }
    };
    let t = &*text_cow;
    let size = crate::layout::font_px(&parent_css, DEFAULT_FONT_SIZE);
    let bold = parent_css.bold.unwrap_or(false);
    let mono = parent_css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);
    let is_ahem =
        crate::layout::measure::font_metrics::uses_ahem(parent_css.font_family.as_deref());
    let lh = crate::inline_box::altura_da_linha(&parent_css, size, ctx.measurer);
    let family = parent_css.font_family.as_deref();
    // Measured in the parent's family, the one the item carries to the
    // painter. It was `text_width(t, size, bold, false, mono)` — no family,
    // and `bold`/`mono` in each other's places.
    let tw = ctx
        .measurer
        .text_width_family(t, size, family, mono, bold, false);
    if avail_w > 0.0 && tw > avail_w {
        if let Some(parent) = dom.node(id).parent {
            let bfc = super::bfc::BlockFormattingContext::new();
            let end_y = crate::layout::inline::line::layout_inline_flow(
                dom,
                parent,
                &[(id, box_id)],
                x,
                y,
                avail_w,
                None,
                &parent_css,
                size,
                &bfc,
                ctx,
                list,
            );
            let total_h = (end_y - y).max(lh);
            if parent_css.writing_mode.map_or(false, |wm| !wm.is_horizontal()) {
                return (total_h, tw.min(avail_w));
            } else {
                return (tw.min(avail_w), total_h);
            }
        }
    }
    let orientation = match parent_css.writing_mode {
        Some(crate::style::WritingMode::VerticalRl | crate::style::WritingMode::SidewaysRl) => {
            crate::paint::item::Orientation::SidewaysRl
        }
        Some(crate::style::WritingMode::VerticalLr | crate::style::WritingMode::SidewaysLr) => {
            crate::paint::item::Orientation::SidewaysLr
        }
        _ => crate::paint::item::Orientation::Horizontal,
    };
    list.push_item(DisplayItem::Text {
        x,
        y,
        text: t.into(),
        color: parent_css.color.unwrap_or(0x000000FF),
        size,
        mono,
        is_ahem,
        family: family.map(Into::into),
        bold,
        italic: false,
        letter_spacing: parent_css.letter_spacing.unwrap_or(0.0),
        decoration: 0,
        orientation,
    });
    let (w, h) = if parent_css.writing_mode.map_or(false, |wm| !wm.is_horizontal()) {
        (lh, tw)
    } else {
        (tw, lh)
    };
    crate::layout::record_box_rect(list, box_id, Rect::new(x, y, w, h));
    let ascent = ctx.measurer.font_ascent_family(size, family);
    crate::layout::inline::line_baseline::register_last_line(id, y + ascent);
    (w, h)
}
