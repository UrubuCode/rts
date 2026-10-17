//! The element's OWN `direction` and `writing-mode`, composed BEFORE its
//! declarations are applied, so that every logical property it carries maps
//! to a physical side through them (CSS Logical Properties §4) — whichever
//! rule, origin or layer happens to declare the pair.
//!
//! Its own module because `dom/cascade.rs` is past the 500-line ceiling and
//! does not grow; it only calls this.
//!
//! The order is the one `Dom::cascade` merges in, strongest first: inline
//! `!important`, author `!important`, the per-node override, inline normal,
//! author normal, and finally what is inherited. The `all: initial` resets
//! are mirrored where `Dom::cascade` resets the style, since a reset layer
//! cannot be the one that answers.

use crate::style::stylesheet::RuleAxes;
use crate::style::{ComputedStyle, DeclBlock, Direction, WritingMode};

pub(in crate::dom) fn compose(
    rules: RuleAxes,
    inline: &DeclBlock,
    override_node: Option<&ComputedStyle>,
    inherited_direction: Option<Direction>,
    inherited_writing_mode: Option<WritingMode>,
) -> (Option<Direction>, Option<WritingMode>) {
    let mut direction = inherited_direction;
    let mut writing_mode = inherited_writing_mode;
    // Weakest first, each layer overwriting what it declares — the same
    // sequence of merges (and resets) as the cascade itself.
    let mut layer = |d: Option<Direction>, w: Option<WritingMode>| {
        direction = d.or(direction);
        writing_mode = w.or(writing_mode);
    };
    if !(inline.all_initial_normal || rules.important_resets || inline.all_initial_important) {
        layer(rules.normal.direction, rules.normal.writing_mode);
    }
    if !(rules.important_resets || inline.all_initial_important) {
        layer(inline.normal.direction, inline.normal.writing_mode);
        if let Some(ov) = override_node {
            layer(ov.direction, ov.writing_mode);
        }
    }
    if !inline.all_initial_important {
        layer(rules.important.direction, rules.important.writing_mode);
    }
    layer(inline.important.direction, inline.important.writing_mode);
    (direction, writing_mode)
}
