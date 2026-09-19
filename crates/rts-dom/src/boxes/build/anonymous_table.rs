//! The anonymous TABLE, generated from the outside in (CSS 2.1 §17.2.1).
//!
//! Rule 3 of that section: a table part whose parent is not what it needs —
//! a `table-cell` outside a row, a `table-row` or row group outside a table —
//! gets an anonymous `table` box around it and around every CONSECUTIVE
//! sibling that is also a table part. The anonymous rows and cells the run
//! may still need INSIDE that table are not generated here: `table/grid.rs`
//! already makes them for a real table, from the table's children, and
//! building them twice would be two answers to one question.
//!
//! Without this, a misparented cell was laid out as the block it is not:
//! `<div><div style="display:table-cell">` stacked every cell at full width
//! where Blink puts them side by side in a shrink-to-fit table
//! (`claude-tabela-anonima-de-fora`), and a floated `table-row` — the shape
//! of the seven WPT `float-applies-to-*` — lost its cells' layout entirely.
//!
//! ## The refusals, stated
//!
//! - **Only a FLOW container wraps.** A flex or grid container blockifies its
//!   children, so a `table-cell` child of one is a flex/grid item and never a
//!   table part. A table, row group or row is not wrapped either: its
//!   misparented children are the table layout's job, and it already does it.
//! - **Not inside an inline.** There the anonymous box would be an
//!   `inline-table`, which this engine does not lay out as inline-level yet.
//!   The run is left as it was.
//! - **Not in a container that split an inline** (`materializa_contentor`).
//!   Both families at once in one container is a case no fixture measures.

use super::{Construcao, NodeIdx};
use crate::boxes::BoxId;
use crate::dom::NodeKind;
use crate::style::DisplayKind;

impl Construcao<'_> {
    /// Descends into `children` of the container `node` (box `id`), wrapping
    /// every run of table parts in an anonymous table when `node` is a flow
    /// container that needs it — and descending plainly otherwise.
    pub(super) fn descend_children(&mut self, node: NodeIdx, id: BoxId, children: Vec<NodeIdx>) {
        if !self.wraps_table_parts(node) || !children.iter().any(|&c| is_table_part_child(self.dom, c)) {
            for child in children {
                self.descend(child, Some(id));
            }
            return;
        }
        let mut i = 0;
        while i < children.len() {
            if !is_table_part_child(self.dom, children[i]) {
                self.descend(children[i], Some(id));
                i += 1;
                continue;
            }
            // The run: table parts, with the collapsible whitespace and
            // comments BETWEEN them. Whitespace after the last part is not the
            // table's and stays outside it.
            let mut fim = i + 1;
            let mut j = i + 1;
            while j < children.len() {
                if is_table_part_child(self.dom, children[j]) {
                    fim = j + 1;
                } else if !is_collapsible_space(self.dom, children[j]) {
                    break;
                }
                j += 1;
            }
            let tabela = self.tree.push_anonymous_table(node, id);
            for &c in &children[i..fim] {
                self.descend(c, Some(tabela));
            }
            i = fim;
        }
    }

    /// Does `node` wrap misparented table parts at all? A flow container that
    /// is not itself a table part, and not an inline box.
    fn wraps_table_parts(&self, node: NodeIdx) -> bool {
        let Some(css) = self.dom.computed_style_idx(node) else { return false };
        let display = css.effective_display();
        // A table cell is a flow container for its CONTENT, although
        // `inner_of` files it under `Table`; a misparented cell inside one is
        // exactly rule 3's case.
        if display == Some(DisplayKind::TableCell) {
            return true;
        }
        if display.is_some_and(DisplayKind::is_table_part) {
            return false;
        }
        let fc = crate::boxes::context::element_formatting_context(self.dom, node);
        fc.inner == crate::boxes::InnerDisplay::Flow && !(fc.is_inline_level() && !fc.independent)
    }
}

/// A table part that needs a table above it: a row group, a row, a cell or a
/// caption. Floats and absolutely positioned boxes are blockified and are none
/// of these, which `effective_display` already says.
fn is_table_part_child(dom: &crate::dom::Dom, node: NodeIdx) -> bool {
    if !matches!(dom.node(node).kind, NodeKind::Element { .. }) {
        return false;
    }
    matches!(
        dom.computed_style_idx(node).and_then(|c| c.effective_display()),
        Some(
            DisplayKind::TableRowGroup
            | DisplayKind::TableHeaderGroup
            | DisplayKind::TableFooterGroup
                | DisplayKind::TableRow
                | DisplayKind::TableCell
                | DisplayKind::TableCaption
        )
    )
}

/// Collapsible whitespace or a comment: what may sit between two table parts
/// of one run without ending it (§17.2.1 rule 1 drops it inside a table).
fn is_collapsible_space(dom: &crate::dom::Dom, node: NodeIdx) -> bool {
    match &dom.node(node).kind {
        NodeKind::Text(t) => t.trim().is_empty(),
        NodeKind::Comment(_) => true,
        _ => false,
    }
}
