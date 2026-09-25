//! What the paint list IS and how it is painted: the pieces of the list and
//! their traversals, stacking order, transforms, and the emitters of borders,
//! scrollbars and backgrounds.
//!
//! `layout/` produces the list and `paint/` defines it; the dependency runs
//! one way (`docs/superpowers/plans/2026-09-25-paint-and-query.md`, FORM 1).

pub(crate) mod background_image;
pub(crate) mod decor;
pub(crate) mod item;
pub(crate) mod list;
pub(crate) mod pieces;
pub(crate) mod stacking;
pub(crate) mod style;
pub(crate) mod transform;

pub use self::decor::{emit_scrollbar, emit_scrollbar_in};
pub use self::item::{Corners, DisplayItem};
pub use self::list::{DisplayList, Rect, ScrollRegion};
pub use self::pieces::Piece;
pub use self::transform::{Mat2d, TransformList, TransformOp, MAX_TRANSFORM_OPS};
