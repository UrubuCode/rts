//! What a box IS in the flow, and what it establishes inside itself.
//!
//! This is the third question a box tree has to answer and the DOM cannot.
//! `display` is one property, but CSS Display 3 reads it as TWO independent
//! answers: how the box behaves towards its SIBLINGS (the outer display) and
//! what formatting context it lays its CHILDREN out in (the inner display).
//! `inline-flex` is the pair that makes the split unavoidable — inline-level
//! outside, flex inside — and every engine that ships keeps them apart for
//! that reason.
//!
//! **Nothing here is stored on the box, and that is the rule of this module.**
//! A box keeps the SOURCE of its style and asks fresh (see `BoxKind`), because
//! a copy taken at build time goes stale the moment a style epoch bumps
//! without a DOM revision. A formatting context derived from that style is the
//! same copy wearing another name, so it is computed on demand from the live
//! `ComputedStyle` every time.
//!
//! **`independent` delegates and `outer` does not, and the difference is the
//! finding that made this module necessary.** `independent` is
//! `layout::bloco::establishes_block_formatting_context`, which carries the
//! float, `overflow`, out-of-flow, flex-item and document-root triggers with
//! the fixtures that pinned each; re-deriving it here would be a second truth.
//!
//! `outer` could not delegate to `layout::caixa::is_block_level`, because that
//! function does not answer this question. It answers "does this element go
//! through `layout_block`", which is a ROUTING answer: an `inline-block` or an
//! `inline-flex` goes through it — they need a block pass to paint their own
//! box — and answers `true` there while being inline-LEVEL to its siblings.
//! Measured: a `<div style="display:inline-flex">` answered `is_block_level ==
//! true`, and the first version of this module reported it as block-level
//! outside, which is the bug the two-value model exists to make impossible.
//!
//! So the two coexist and mean different things. `is_block_level` stays the
//! engine's routing question at its 46 sites; `outer` is the CSS question, and
//! it reads the declared `display` first and falls back to the tag's own
//! default. Where they disagree — `inline-block`, `inline-flex` — the
//! disagreement is the point rather than a drift, and any future merge of the
//! two has to start here.
//!
//! What IS new here is the question that needs a tree to answer at all:
//! whether a block container's children make it an INLINE formatting context
//! or a block one. CSS 2.1 section 9.2.1 decides that by looking at the
//! children, and the DOM cannot be asked — an anonymous box has no node, and a
//! box's children are not its node's children once the block-in-inline split
//! has run.

use super::{BoxId, BoxKind, BoxTree};
use crate::dom::Dom;
use crate::style::DisplayKind;

/// How a box behaves towards its siblings: does it take a line of its own, or
/// does it flow in one beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OuterDisplay {
    /// Block-level: it starts on a new line and its siblings stack under it.
    Block,
    /// Inline-level: it flows in a line with its siblings. This covers the
    /// ATOMIC inline-level boxes too — `inline-block`, `inline-flex`, a
    /// replaced element — because "inline-level" is a statement about the
    /// outside only, and what they do inside is `inner`'s answer.
    Inline,
}

/// The formatting context a box lays its own children out in.
///
/// `Flow` is deliberately ONE variant and not two. Whether a flow container
/// runs a block formatting context or an inline one is decided by its
/// CHILDREN and not by its `display`, so it is not a property of this value;
/// [`BoxTree::runs_inline_formatting_context`] is the question, and it needs
/// the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InnerDisplay {
    /// Normal flow: block boxes stacking, or a line box, per the children.
    Flow,
    /// Flex container (Flexbox section 4). `inline-flex` lands here as well:
    /// it is flex INSIDE and only its `outer` differs.
    Flex,
    /// Grid container.
    Grid,
    /// Table formatting, including the internal parts (`table-row`, `-cell`).
    Table,
}

/// The pair, plus whether the box contains its floats and margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormattingContext {
    /// What it is to its siblings.
    pub outer: OuterDisplay,
    /// What it lays its children out in.
    pub inner: InnerDisplay,
    /// `true` when the box establishes a formatting context INDEPENDENT of its
    /// parent's: its floats do not escape, its margins do not collapse
    /// through, and a sibling float outside does not intrude into it.
    ///
    /// This is not derivable from `inner` alone, which is why it is a third
    /// field and not a method: `overflow: hidden` on a plain `display: block`
    /// establishes one while leaving both halves of the pair untouched.
    pub independent: bool,
}

impl FormattingContext {
    /// `true` when the box takes a line of its own.
    pub fn is_block_level(self) -> bool {
        self.outer == OuterDisplay::Block
    }

    /// `true` when the box flows in a line with its siblings.
    pub fn is_inline_level(self) -> bool {
        self.outer == OuterDisplay::Inline
    }

    /// `true` when the box is inline-level to its siblings but opaque inside —
    /// `inline-block`, `inline-flex`, a replaced element. A line box treats it
    /// as one unbreakable item, never descending into it to find break
    /// opportunities, and THAT is the reason this question exists separately
    /// from [`is_inline_level`](Self::is_inline_level).
    pub fn is_atomic_inline(self) -> bool {
        self.is_inline_level() && (self.inner != InnerDisplay::Flow || self.independent)
    }
}

/// The inner half of `display`, per CSS Display 3 section 2.
fn inner_of(display: DisplayKind) -> InnerDisplay {
    match display {
        DisplayKind::Flex
        | DisplayKind::FlexWrap
        | DisplayKind::InlineFlex
        | DisplayKind::InlineFlexWrap => InnerDisplay::Flex,
        DisplayKind::Grid | DisplayKind::InlineGrid => InnerDisplay::Grid,
        DisplayKind::Table
        | DisplayKind::InlineTable
        | DisplayKind::TableRowGroup
        | DisplayKind::TableHeaderGroup
        | DisplayKind::TableFooterGroup
        | DisplayKind::TableRow
        | DisplayKind::TableCell
        | DisplayKind::TableCaption => InnerDisplay::Table,
        // `block`, `inline`, `inline-block`, `list-item` and `none` are all
        // FLOW inside. `inline-block` differs from `inline` on the outside and
        // in `independent`, never here — which is exactly the confusion the
        // two-value model exists to end.
        _ => InnerDisplay::Flow,
    }
}

/// The formatting context of an ELEMENT, without needing a box.
///
/// Public to the crate because `build` needs it BEFORE the box exists: the
/// block-in-inline split has to know whether the element it is descending into
/// is a real inline box, and at that point it has a node and nothing else.
///
/// `outer` reads the declared display first and the tag default second, which
/// is the cascade order CSS gives. An element with no computed style answers
/// block-level flow; `build_mirror` gives it no box at all, so the value is
/// only ever seen by a caller holding a hand-built tree.
pub(crate) fn element_formatting_context(dom: &Dom, node: crate::dom::NodeIdx) -> FormattingContext {
    let declared = dom
        .computed_style_idx(node)
        .and_then(|css| css.effective_display());
    let outer = match declared {
        // `display: none` generates no box; when one is asked about anyway it
        // answers block-level, which is what the absent box would have been.
        Some(DisplayKind::None) | None => {
            if tag_is_block_level(dom, node) {
                OuterDisplay::Block
            } else {
                OuterDisplay::Inline
            }
        }
        Some(d) if d.is_inline_level() => OuterDisplay::Inline,
        Some(_) => OuterDisplay::Block,
    };
    let inner = declared.map_or(InnerDisplay::Flow, inner_of);
    let independent = dom.computed_style_idx(node).is_some_and(|css| {
        crate::layout::bloco::establishes_block_formatting_context(dom, node, &css)
    });
    FormattingContext {
        outer,
        inner,
        independent,
    }
}

/// The formatting context of a GENERATED box, from the pseudo's OWN style —
/// `css` — with the ORIGINATING element's style `pai` as its parent box's.
///
/// `outer` has no tag to fall back on: a pseudo that declares no `display`
/// takes the initial value, `inline` (CSS Display 3 §2), which is also what
/// `layout/pseudo_inline.rs` does with one. `independent` asks the same
/// style triggers an element asks (`layout::bfc_estilo`), with the parent
/// being the originating element — a pseudo of a flex container is a flex
/// item. The two node-only triggers do not apply: a pseudo is never the
/// document root, and its `overflow` never propagates to the viewport.
///
/// A box the cascade stopped generating (`css` is `None`) answers an inline
/// box, which is what it was by default; the memo key of `dom/box_tree.rs`
/// keeps that from happening inside one pass.
fn generated_formatting_context(
    css: Option<&crate::style::ComputedStyle>,
    pai: Option<&crate::style::ComputedStyle>,
) -> FormattingContext {
    let declared = css.and_then(|c| c.effective_display());
    let outer = match declared {
        Some(d) if d != DisplayKind::None && !d.is_inline_level() => OuterDisplay::Block,
        _ => OuterDisplay::Inline,
    };
    let independent = css.is_some_and(|c| {
        crate::layout::bfc_estilo::pelo_estilo(c, pai) || crate::layout::bfc_estilo::overflow_estabelece(c)
    });
    FormattingContext {
        outer,
        inner: declared.map_or(InnerDisplay::Flow, inner_of),
        independent,
    }
}

/// The tag's own default: block-level for the tags the HTML default sheet
/// makes block, inline for everything else. `crate::block::lookup` is that
/// sheet, and asking it here rather than keeping a list is what stops a second
/// answer to "which tags are blocks" existing.
fn tag_is_block_level(dom: &Dom, node: crate::dom::NodeIdx) -> bool {
    match &dom.node(node).kind {
        crate::dom::NodeKind::Element { tag } => crate::block::lookup(tag).is_some(),
        _ => false,
    }
}

impl BoxTree {
    /// The formatting context of one box, computed from the LIVE style.
    ///
    /// A box with no element behind it answers from its kind rather than from
    /// a style:
    ///
    /// - a TEXT box is inline-level flow, always. Text has no `display` of its
    ///   own and cannot be blockified; what it inherits from the enclosing
    ///   element is colour and font, never the box type.
    /// - an ANONYMOUS box is block-level flow, always. It exists because the
    ///   block-in-inline rule split an inline around a block-level child, and
    ///   the box it creates to hold the inline run is a block box by
    ///   definition — taking the split inline's `display: inline` here would
    ///   recreate the very nesting the split undid.
    /// - a GENERATED box answers from the pseudo's own `display`, never from
    ///   its originating element's — see [`generated_formatting_context`]. It
    ///   counts in [`Self::runs_inline_formatting_context`] like any child:
    ///   CSS 2.1 §12.1 inserts it among the element's children, so a
    ///   `::before { display: block }` does make its container a stack.
    pub fn formatting_context(&self, dom: &Dom, id: BoxId) -> FormattingContext {
        match self.kind(id) {
            BoxKind::Text { .. } => FormattingContext {
                outer: OuterDisplay::Inline,
                inner: InnerDisplay::Flow,
                independent: false,
            },
            BoxKind::Anonymous { role: super::AnonymousRole::Block, .. } => FormattingContext {
                outer: OuterDisplay::Block,
                inner: InnerDisplay::Flow,
                independent: false,
            },
            // An anonymous TABLE is `display: table`: block-level (its parent is
            // a flow container — `build.rs` never wraps inside an inline) and a
            // table inside, which contains its own floats and margins.
            BoxKind::Anonymous { role: super::AnonymousRole::Table, .. } => FormattingContext {
                outer: OuterDisplay::Block,
                inner: InnerDisplay::Table,
                independent: true,
            },
            BoxKind::Element(node) => element_formatting_context(dom, node),
            BoxKind::Generated { originating, .. } => {
                generated_formatting_context(self.style(dom, id).as_deref(), dom.computed_style_idx(originating).as_deref())
            }
        }
    }

    /// `true` when this box lays its children out as LINES rather than as a
    /// stack of blocks — CSS 2.1 section 9.2.1's rule that a block container
    /// holding only inline-level content runs an inline formatting context.
    ///
    /// **This is the question that needs the tree.** It is decided by the
    /// children, and after the block-in-inline split a box's children are not
    /// its node's children: the split replaced part of them with anonymous
    /// boxes. Asking the DOM gives the answer for a shape that no longer
    /// exists.
    ///
    /// A box with no children answers `true`: an empty block container holds
    /// no block-level content, and the engine treats it as an empty line box
    /// rather than as an empty stack. The two agree on the geometry, and this
    /// is the one the rest of the flow already assumes.
    ///
    /// A box that is not `Flow` inside answers `false` — a flex or grid
    /// container runs its own algorithm and never a line box, whatever its
    /// children are.
    pub fn runs_inline_formatting_context(&self, dom: &Dom, id: BoxId) -> bool {
        if self.formatting_context(dom, id).inner != InnerDisplay::Flow {
            return false;
        }
        self.children(id)
            .iter()
            .all(|&child| self.formatting_context(dom, child).is_inline_level())
    }
}
