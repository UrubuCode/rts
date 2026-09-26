//! What is ASKED of a paint list after layout: the geometry per node and per
//! box, `getBoundingClientRect`'s rect, and the hit-test.
//!
//! It reads `paint/` and the box tree; `layout/` never reads it
//! (`docs/superpowers/plans/2026-09-25-paint-and-query.md`, FORM 1).

pub(crate) mod geometry;
pub(crate) mod hit;
pub(crate) mod rect;
#[cfg(test)]
mod tests;

pub use self::geometry::Geometry;
