//! Real text: fonts resolved from a CSS family list, their metrics with
//! Blink's rounding, shaping through `rustybuzz`, UAX #14 break opportunities,
//! grapheme clusters, and glyph coverage bitmaps.
//!
//! The API speaks in its own terms — families, faces, runs, glyphs, bitmaps —
//! and never in the layout's: what a `font-family` value MEANS, where a line
//! breaks and which box a run belongs to are `rts-dom`'s decisions. See
//! `README.md` for the rules and `PLAN.md` for what comes next.

pub mod breaks;
pub mod face;
pub mod fonts;
pub mod raster;
pub mod shape;

#[cfg(feature = "dom-measurer")]
pub mod adapter;

pub use breaks::{grapheme_boundaries, line_break_opportunities};
pub use face::Face;
pub use fonts::{FontStore, Style};
pub use raster::{Bitmap, rasterise};
pub use shape::{CacheStats, Glyph, cache_stats, shape, shaped_width};
