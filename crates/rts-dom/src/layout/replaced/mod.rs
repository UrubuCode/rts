//! Replaced elements — img, svg, canvas, input, select: their intrinsic
//! and transferred sizes.

use super::*;

pub(super) mod input;
pub(super) mod input_sizing;
pub(super) mod replaced;
pub(super) mod select;
pub(super) mod transferred_size;

pub(in crate::layout) use replaced::ForcedReplacedScope;
