//! The flattened items a container's children resolve to, and the questions
//! the build asks of them: which are block-level, and which runs hold inline
//! content that an anonymous block box must enclose (CSS 2.1 §9.2.1.1).
//!
//! Split out of `build.rs` when BT-3 added the second anonymous-block family
//! and took that file past the ceiling; the construction stays there.

use crate::dom::{Dom, NodeIdx, NodeKind};
use crate::style::DisplayKind;

/// `true` for an item that is block-level to its siblings inside the container.
///
/// A `Block` came out of a split and is block-level by construction; a `Plain`
/// is asked the ordinary question, because a container that split may also hold
/// block children of its own; a `Fragment` is a piece of an inline element and
/// is never block-level.
pub(super) fn e_item_de_bloco(dom: &Dom, item: &FlowItem) -> bool {
    match *item {
        FlowItem::Block(_) => true,
        FlowItem::Plain(n) => is_block_level_child(dom, n),
        FlowItem::Fragment { .. } => false,
    }
}

/// `true` when a run holds nothing that could paint: only whitespace-only text
/// and comments.
///
/// `white-space` is asked of the element the text sits DIRECTLY in — the
/// container for its own text, the inline for a fragment's — because it is
/// inherited and applies per inline box (CSS Text 3 §3): a `pre` span keeps
/// its spaces inside a `normal` container, and a `normal` span collapses its
/// own inside a `<pre>` (asking only the container got both wrong).
pub(super) fn corrida_so_de_espaco(dom: &Dom, contentor: NodeIdx, itens: &[FlowItem]) -> bool {
    let preserves = preserves_spaces(dom, contentor);
    itens.iter().all(|item| item_so_de_espaco(dom, item, preserves))
}

pub(super) fn preserves_spaces(dom: &Dom, no: NodeIdx) -> bool {
    dom.computed_style_idx(no)
        .and_then(|c| c.white_space)
        .is_some_and(|w| w.preserves_spaces())
}

/// `true` when a run holds no inline content at all: only what
/// [`corrida_so_de_espaco`] accepts, plus floats and absolutely positioned
/// elements.
///
/// Those are block-level out-of-flow boxes (§9.3), not inline content, and
/// Blink leaves them as direct children of the block flow — where the flow's
/// float arm already placed them. Wrapping them would ALSO let the margins of
/// the blocks on either side collapse through a zero-height anonymous box,
/// where that arm deliberately closes the collapse: an answer moving for a
/// reason BT-3 does not measure. A float WITH text in its run stays in it —
/// that one is in the middle of a line (`float_in_line.rs`).
pub(super) fn run_without_line_content(dom: &Dom, container: NodeIdx, items: &[FlowItem]) -> bool {
    if corrida_so_de_espaco(dom, container, items) {
        return true;
    }
    let preserves = preserves_spaces(dom, container);
    !preserves
        && items.iter().all(|i| match i {
            FlowItem::Plain(n) if out_of_flow(dom, *n) => true,
            other => item_so_de_espaco(dom, other, preserves),
        })
}

/// The same question for ONE item, recursing into a fragment.
///
/// A fragment has to be entered rather than refused outright: the whitespace of
/// `<span> <div/> </span>` lives inside the span, not beside it, so a test that
/// only looks at `Plain` answers `false` and wraps a whole empty line between
/// the two blocks. That was this function's behaviour and the test above caught
/// it.
///
/// **But a fragment whose element PAINTS is not empty**, however little text it
/// holds: a `<span>` with a background, a border or padding draws something on
/// that line, and the anonymous box is what gives it a line to draw on. The
/// question "does this inline paint despite being inline" is one the engine
/// already answers in `inline_box`, and asking it here rather than re-deriving
/// it is what keeps the two from drifting.
pub(super) fn item_so_de_espaco(dom: &Dom, item: &FlowItem, preserves: bool) -> bool {
    match item {
        FlowItem::Plain(n) => match &dom.node(*n).kind {
            NodeKind::Text(t) => !preserves && t.trim().is_empty(),
            NodeKind::Comment(_) => true,
            _ => false,
        },
        FlowItem::Fragment { node, items } => {
            let pinta = dom
                .computed_style_idx(*node)
                .is_some_and(|css| crate::inline_box::cria_caixa_apesar_de_inline(&css));
            let preserves = preserves_spaces(dom, *node);
            !pinta && items.iter().all(|i| item_so_de_espaco(dom, i, preserves))
        }
        FlowItem::Block(_) => false,
    }
}

/// `true` for an element that is an inline BOX: inline-level to its siblings,
/// flow inside, and establishing no formatting context of its own. Only a box
/// like this can be split by a block-level descendant (CSS 2.1 §9.2.1.1) —
/// `inline-block` and `inline-flex` pass the outer test and fail the other two,
/// and a block-level child inside one of those is ordinary content.
///
/// This used to ask `!is_block_level`, which answered the question by accident:
/// that function routes to `layout_block`, and an `inline-block` routes there, so
/// it fell out of the criterion for a reason unrelated to what the criterion
/// means. `context.rs` carries the divergence in full. Asking
/// `effective_display() == Some(Inline)` was tried before that and never fired at
/// all — a plain `<span>` declares no display, inline being the tag default, so
/// the test was against `None` every time.
pub(super) fn is_inline_flow_box(dom: &Dom, node: NodeIdx) -> bool {
    if !matches!(&dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    if dom.computed_style_idx(node).is_none() {
        return false;
    }
    let fc = crate::boxes::context::element_formatting_context(dom, node);
    fc.is_inline_level() && fc.inner == crate::boxes::InnerDisplay::Flow && !fc.independent
}

/// `true` for an ELEMENT child that is an IN-FLOW block-level box — the one
/// question the split needs about a DIRECT child, asked through
/// `element_formatting_context` so that "what is this to its siblings" has a
/// single answer in this crate. A non-element is never block-level: a text node
/// stays in the inline run, and a comment generates no box at all.
///
/// `display: none` is excluded because it generates no box: a child that does not
/// exist cannot split anything, and counting it would produce an anonymous box
/// around nothing.
///
/// **A float or an absolutely positioned box is excluded too, although it IS
/// block-level.** §9.2.1.1 splits around "an in-flow block-level box", and both
/// are out of flow (§9.3); `effective_display` blockifies them, so asking only
/// the outer display split `<span>a<div style="float:left"/>b</span>` in three
/// and put `b` on a line of its own, where Blink keeps one line shortened
/// around the float. Such a child stays in the inline run as ordinary content —
/// which is also where the inline flow already knew how to place it.
pub(super) fn is_block_level_child(dom: &Dom, node: NodeIdx) -> bool {
    if !matches!(&dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    let Some(css) = dom.computed_style_idx(node) else {
        return crate::boxes::context::element_formatting_context(dom, node).is_block_level();
    };
    if css.effective_display() == Some(DisplayKind::None) {
        return false;
    }
    !out_of_flow(dom, node)
        && crate::boxes::context::element_formatting_context(dom, node).is_block_level()
}

/// `true` for an element that is floated or absolutely positioned.
pub(super) fn out_of_flow(dom: &Dom, node: NodeIdx) -> bool {
    if !matches!(&dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    dom.computed_style_idx(node).is_some_and(|css| {
        css.effective_display() != Some(DisplayKind::None)
            && (css.float_side.is_some_and(|f| f != crate::style::FloatSide::None)
                || css.position.is_some_and(|p| p.out_of_flow()))
    })
}

/// One item of the flattened sequence a splitting inline's children resolve to,
/// in document order.
///
/// `Fragment` is what gives a split inline SEVERAL boxes rather than one: each
/// run of inline-level content between (or beside) the block-level children it
/// encloses becomes its own `Fragment`, and each `Fragment` for the same node is
/// a separate `push_element` call — which is what lets `by_node` list every one
/// of them.
pub(super) enum FlowItem {
    /// A block-level child, promoted here from however deep inside the nested
    /// inlines it sat. It is materialised as a sibling of the anonymous boxes
    /// around it, never as a child of one.
    Block(NodeIdx),
    /// One run of `node`'s own inline-level content.
    ///
    /// `items` nest instead of flattening further, so that a nested inline with
    /// nothing to bubble past THIS run keeps its own fragment as a child of this
    /// one — only the actual block-level content rises past every enclosing
    /// inline, all the way to the outermost splitting ancestor's container.
    Fragment { node: NodeIdx, items: Vec<FlowItem> },
    /// Ordinary content: text, a comment, or an element that needs no special
    /// handling at this level. It may still turn out to split ITS OWN
    /// descendants — that is decided when the descent reaches it.
    Plain(NodeIdx),
}

/// Wraps the accumulated run into one `Fragment` of `node`, or does nothing when
/// the run is empty — see the "no empty fragment" note on
/// [`Construcao::itens_do_inline`].
pub(super) fn fecha_fragmento(node: NodeIdx, run: &mut Vec<FlowItem>, out: &mut Vec<FlowItem>) {
    if run.is_empty() {
        return;
    }
    out.push(FlowItem::Fragment {
        node,
        items: std::mem::take(run),
    });
}
