//! The triggers of a block formatting context that the STYLE alone decides
//! (CSS 2.1 §9.4.1), split out of `block::establishes_block_formatting_context`.
//!
//! They were one function keyed by a `NodeIdx`, and a box with no node could
//! not ask it: a generated box (`::before`/`::after`, lot BT-5) has a style and
//! a parent but no node. Copying the list of triggers into `boxes::context`
//! would have been a second answer to "does this establish a BFC", so the
//! style half moved here and both callers ask it. What stays in `block.rs` is
//! what only a NODE can answer: whether it is the document root, and whether
//! its `overflow` propagates to the viewport instead of applying to itself.

use crate::style::{ComputedStyle, DisplayKind, FloatSide};

/// `true` when `css`, as the style of a box whose parent box has style
/// `parent_css`, establishes its own block formatting context by the style alone —
/// every trigger except the document root and `overflow`, which the caller
/// adds with [`overflow_estabelece`] once it knows `overflow` applies here.
pub(crate) fn pelo_estilo(css: &ComputedStyle, parent_css: Option<&ComputedStyle>) -> bool {
    let display_bfc = matches!(
        css.effective_display(),
        Some(
            DisplayKind::Flex
                | DisplayKind::FlexWrap
                | DisplayKind::InlineFlex // flex por dentro (Flexbox §4): mesmo contexto
                | DisplayKind::InlineFlexWrap
                | DisplayKind::Grid
                | DisplayKind::InlineGrid
                | DisplayKind::InlineTable
                | DisplayKind::InlineBlock
                | DisplayKind::Table
                | DisplayKind::TableRowGroup
                | DisplayKind::TableHeaderGroup
                | DisplayKind::TableFooterGroup
                | DisplayKind::TableRow
                | DisplayKind::TableCell
                | DisplayKind::TableCaption
        )
    );
    let float_bfc = css.float_side.is_some_and(|side| side != FloatSide::None);
    let positioned_bfc = css.position.map(|position| position.out_of_flow()).unwrap_or(false);
    // Um ITEM de flex ou de grid estabelece o seu próprio contexto (Flexbox
    // §4, Grid §6): contém os seus floats como um `flow-root`. Sem isto o
    // `<header class="mb-auto">` do Bootstrap cover — um `float-md-start` e um
    // `float-md-end` lá dentro — media 0px onde o Blink dá 36
    // (`claude-flex-item-contem-floats`).
    let item_bfc = parent_css.is_some_and(|pc| {
        matches!(
            pc.effective_display(),
            Some(
                DisplayKind::Flex
                    | DisplayKind::FlexWrap
                    | DisplayKind::InlineFlex // idem: filho de flex
                    | DisplayKind::InlineFlexWrap
                    | DisplayKind::Grid
                    | DisplayKind::InlineGrid
            )
        )
    });
    css.flow_root.unwrap_or(false) || display_bfc || item_bfc || float_bfc || positioned_bfc
}

/// CSS2.1 §9.4.1: "overflow" outro que não `visible` estabelece um BFC —
/// `scrollable()` (auto/scroll) OU `clips()` (hidden/clip, CSS Overflow 3;
/// `clip` entrou no lote `flex-min-auto-content`, retrabalho: antes de
/// `Overflow::Clip` existir como variante própria, `hidden`/`clip` eram a
/// MESMA e este `any` já os cobria os dois sem saber).
pub(crate) fn overflow_estabelece(css: &ComputedStyle) -> bool {
    [css.overflow_x, css.overflow_y]
        .into_iter()
        .any(|value| value.is_some_and(|o| o.scrollable() || o.clips()))
}
