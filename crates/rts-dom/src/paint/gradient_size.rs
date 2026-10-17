//! Applies `background-size` to `DisplayItem::GradientRect` items in a `DisplayList`.

use crate::dom::Dom;
use crate::paint::item::DisplayItem;
use crate::paint::list::DisplayList;
use crate::paint::pieces::Piece;
use crate::style::{BgSize, ResolveCtx};

pub(crate) fn apply_gradient_sizes(list: &mut DisplayList, dom: &Dom) {
    let len = list.pieces.len();
    for i in 0..len {
        let bid = match &list.pieces[i] {
            Piece::Item(DisplayItem::GradientRect { .. }) => {
                let mut found = None;
                for offset in 1..=4 {
                    if let Some(Piece::Rect(b)) = list.pieces.get(i + offset) {
                        found = Some(*b);
                        break;
                    }
                }
                if found.is_none() {
                    for offset in 1..=4 {
                        if i >= offset {
                            if let Some(Piece::Rect(b)) = list.pieces.get(i - offset) {
                                found = Some(*b);
                                break;
                            }
                        }
                    }
                }
                found
            }
            _ => None,
        };

        if let Some(bid) = bid {
            if let Some(node) = dom.box_tree().node_of(bid) {
                if let Some(css) = dom.computed_style_idx(node) {
                    if let Some(BgSize::Len(sw, sh)) = css.bg_size {
                        if let Piece::Item(DisplayItem::GradientRect { rect, .. }) = &mut list.pieces[i] {
                            let font = crate::layout::font_px(&css, 16.0);
                            let rc = ResolveCtx {
                                parent_content_w: rect.w,
                                node_font_size: font,
                                root_font_size: crate::style::root_font_size(),
                                viewport_w: 1280.0,
                                viewport_h: 800.0,
                            };
                            let gw = match sw {
                                crate::style::Dimension::Auto => rect.w,
                                crate::style::Dimension::Percent(p) => rect.w * p / 100.0,
                                other => other.resolve(&rc).unwrap_or(rect.w),
                            };
                            let gh = match sh {
                                crate::style::Dimension::Auto => rect.h,
                                crate::style::Dimension::Percent(p) => rect.h * p / 100.0,
                                other => other.resolve(&rc).unwrap_or(rect.h),
                            };
                            rect.w = gw;
                            rect.h = gh;
                        }
                    }
                }
            }
        }
    }
}
