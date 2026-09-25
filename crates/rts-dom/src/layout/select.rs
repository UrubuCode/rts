//! A caixa fechada de um `<select>` é um controlo substituído: os `<option>`
//! pertencem ao popup, não ao fluxo nem à altura da caixa.

use super::*;

const NATURAL_W: f32 = 22.0;
const NATURAL_H: f32 = 19.0;

struct Frame {
    margin_left: f32,
    margin_right: f32,
    margin_top: f32,
    margin_bottom: f32,
    horizontal: f32,
    vertical: f32,
    border: f32,
}

fn frame(css: &ComputedStyle, avail_w: f32, ctx: &LayoutCtx) -> (Frame, ResolveCtx) {
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_px(css, DEFAULT_FONT_SIZE),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let margin_left = css.margin.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = css.margin.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = css.margin.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = css.margin.bottom.resolve(&resolve).unwrap_or(0.0);
    let horizontal = css.padding.left.resolve(&resolve).unwrap_or(0.0).max(0.0)
        + css.padding.right.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let vertical = css.padding.top.resolve(&resolve).unwrap_or(0.0).max(0.0)
        + css.padding.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let border = css.border_width.unwrap_or(0.0).max(0.0);
    (
        Frame {
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
            horizontal,
            vertical,
            border,
        },
        resolve,
    )
}

/// Conteúdo natural sem os vãos que `intrinsic_outer_width` soma depois.
/// A caixa mínima 22×19 é a seta do dropdown vazio, medida no Blink.
pub(in crate::layout) fn natural_content(css: &ComputedStyle, ctx: &LayoutCtx) -> (f32, f32) {
    let (f, _) = frame(css, f32::INFINITY, ctx);
    (
        (NATURAL_W - f.horizontal - 2.0 * f.border).max(0.0),
        (NATURAL_H - f.vertical - 2.0 * f.border).max(0.0),
    )
}

pub(in crate::layout) fn layout_select(
    caixa: crate::boxes::BoxId,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let (f, resolve) = frame(css, avail_w, ctx);
    let (natural_w, natural_h) = natural_content(css, ctx);
    let border_box = css.border_box.unwrap_or(false);
    let content_w = forced_outer_w
        .map(|w| (w - f.margin_left - f.margin_right - f.horizontal - 2.0 * f.border).max(0.0))
        .or_else(|| css.width.and_then(|d| d.resolve(&resolve)).map(|w| {
            if border_box { (w - f.horizontal - 2.0 * f.border).max(0.0) } else { w }
        }))
        .unwrap_or(natural_w);
    let content_h = forced_outer_h
        .map(|h| (h - f.margin_top - f.margin_bottom - f.vertical - 2.0 * f.border).max(0.0))
        .or_else(|| super::posicionado::resolve_height(css.height, avail_h, &resolve).map(|h| {
            if border_box { (h - f.vertical - 2.0 * f.border).max(0.0) } else { h }
        }))
        .unwrap_or(natural_h);
    let rect = Rect::new(
        x + f.margin_left,
        y + f.margin_top,
        content_w + f.horizontal + 2.0 * f.border,
        content_h + f.vertical + 2.0 * f.border,
    );
    record_box_rect(list, caixa, rect);
    let opacity = css.opacity.unwrap_or(1.0);
    list.push_item(DisplayItem::SolidRect {
        rect,
        color: apply_opacity(css.bg.unwrap_or(0xFFFFFFFF), opacity),
        radius: Corners::from_style(css, 0.0),
    });
    if f.border > 0.0 {
        list.push_item(DisplayItem::Border {
            rect,
            width: f.border,
            color: apply_opacity(css.border_color.unwrap_or(0x767676FF), opacity),
            radius: css.corner_radius.unwrap_or(0.0),
        });
    }
    (
        rect.w + f.margin_left + f.margin_right,
        rect.h + f.margin_top + f.margin_bottom,
    )
}
