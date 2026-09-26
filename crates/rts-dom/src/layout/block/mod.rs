//! The block formatting context: a block's box, the vertical flow,
//! margin collapse, BFC, the child sequence.

use super::*;

pub(super) mod bare_text;
pub(super) mod bfc;
pub(super) mod bfc_avoids_float;
pub(crate) mod bfc_style;
pub(crate) mod block;
pub(super) mod block_box;
pub(crate) mod box_kind;
pub(super) mod escaped_margin;
pub(super) mod margin_collapse;
pub(super) mod overflow_viewport;
pub(super) mod pseudo_block;
pub(super) mod pseudo_box;
pub(crate) mod rotated;
pub(super) mod rtl;
pub(super) mod sequence;
pub(super) mod vertical_flow;

// `boxes/context.rs` asks this question by the folder path.
pub(crate) use self::block::establishes_block_formatting_context;
