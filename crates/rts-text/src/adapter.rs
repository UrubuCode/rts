//! `RealMeasurer`: `rts_dom::layout::TextMeasurer` answered from real faces.
//!
//! Behind the `dom-measurer` feature because it is the only place this crate
//! names `rts-dom` (plan, F2): the window and the headless instrument both
//! reach it here, so there is one implementation and not two.
//!
//! The fallback is PER CALL: when a family list resolves to no face, the
//! answer is exactly what `ApproxMeasurer` answers for the same arguments, so
//! a machine without the fonts (CI on Linux) lays out as it did before this
//! crate existed, and never at zero width.
//!
//! Ahem's VERTICAL metrics are also `ApproxMeasurer`'s: `rts-dom` defines them
//! unrounded (0.8/0.2 em, `style::ahem`), where rounding the face's own
//! `hhea` would move a baseline by a fraction at sizes not divisible by 5.
//! That is a CSS decision already taken in `rts-dom`, and this adapter asks
//! it rather than taking a second one. Its widths come from shaping — every
//! Ahem glyph advances 1 em, so the two agree.

use std::sync::Arc;

use rts_dom::layout::{ApproxMeasurer, TextMeasurer};

use crate::face::Face;
use crate::fonts::{FontStore, Style};
use crate::shape::shaped_width;

pub struct RealMeasurer {
    store: Arc<FontStore>,
}

impl RealMeasurer {
    pub fn new(store: Arc<FontStore>) -> RealMeasurer {
        RealMeasurer { store }
    }

    pub fn store(&self) -> &FontStore {
        &self.store
    }

    /// The face for a computed family list; with no list, `mono` chooses
    /// between Blink's two defaults as `ApproxMeasurer` does.
    ///
    /// Public so a painter draws with the face that decided the width — the
    /// window (rts-egui) hands its bytes (`Face::data`) to its own renderer.
    /// Resolving again outside this rule is the second answer F4 forbids.
    pub fn face(&self, family: Option<&str>, mono: bool, bold: bool, italic: bool) -> Option<Arc<Face>> {
        let list = family.unwrap_or(if mono { "monospace" } else { "serif" });
        let style = if italic { Style::Italic } else { Style::Normal };
        self.store.resolve(list, if bold { 700 } else { 400 }, style)
    }

    /// The face for vertical metrics, `None` when `ApproxMeasurer` answers.
    fn metrics_face(&self, family: Option<&str>) -> Option<Arc<Face>> {
        self.face(family, false, false, false).filter(|f| !rts_dom::style::is_ahem_family(f.family()))
    }
}

impl TextMeasurer for RealMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32 {
        self.text_width_family(text, size, None, mono, bold, italic)
    }

    fn text_width_family(&self, text: &str, size: f32, family: Option<&str>, mono: bool, bold: bool, italic: bool) -> f32 {
        match self.face(family, mono, bold, italic) {
            Some(face) => shaped_width(&face, text, size, true),
            None => ApproxMeasurer.text_width_family(text, size, family, mono, bold, italic),
        }
    }

    fn line_height(&self, size: f32) -> f32 {
        self.line_height_family(size, None)
    }

    fn line_height_family(&self, size: f32, family: Option<&str>) -> f32 {
        match self.metrics_face(family) {
            Some(face) => face.normal_line_height(size),
            None => ApproxMeasurer.line_height_family(size, family),
        }
    }

    fn font_ascent(&self, size: f32) -> f32 {
        self.font_ascent_family(size, None)
    }

    fn font_ascent_family(&self, size: f32, family: Option<&str>) -> f32 {
        match self.metrics_face(family) {
            Some(face) => face.ascent_px(size),
            None => ApproxMeasurer.font_ascent_family(size, family),
        }
    }

    fn font_descent(&self, size: f32) -> f32 {
        self.font_descent_family(size, None)
    }

    fn font_descent_family(&self, size: f32, family: Option<&str>) -> f32 {
        match self.metrics_face(family) {
            Some(face) => face.descent_px(size),
            None => ApproxMeasurer.font_descent_family(size, family),
        }
    }

    fn identity(&self) -> u64 {
        // Never 0: that is the stateless default, and ApproxMeasurer's.
        self.store.identity() | 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no system directory and nothing registered, every answer is
    /// ApproxMeasurer's — the Linux CI path.
    #[test]
    fn a_family_with_no_face_answers_what_approx_answers() {
        let m = RealMeasurer::new(Arc::new(FontStore::with_system_dir(None)));
        for fam in [None, Some("serif"), Some("NoSuchFont, monospace"), Some("Ahem")] {
            assert_eq!(m.text_width_family("AVATAR Toy", 16.0, fam, false, true, false), ApproxMeasurer.text_width_family("AVATAR Toy", 16.0, fam, false, true, false));
            assert_eq!(m.line_height_family(13.0, fam), ApproxMeasurer.line_height_family(13.0, fam));
            assert_eq!(m.font_ascent_family(13.0, fam), ApproxMeasurer.font_ascent_family(13.0, fam));
            assert_eq!(m.font_descent_family(13.0, fam), ApproxMeasurer.font_descent_family(13.0, fam));
        }
    }

    /// With the real faces, widths are kerned and metrics match the tables;
    /// a registered Ahem keeps rts-dom's unrounded metrics.
    #[test]
    fn real_faces_kern_and_ahem_keeps_its_css_metrics() {
        let store = Arc::new(FontStore::new());
        if store.resolve("serif", 400, Style::Normal).is_none() {
            eprintln!("SKIPPED: no Times New Roman on this machine");
            return;
        }
        let m = RealMeasurer::new(store.clone());
        assert_eq!(m.line_height(16.0), 18.0);
        assert_eq!(m.font_ascent_family(16.0, Some("monospace")), 15.0);
        let kerned = m.text_width("AVATAR Toy To.", 16.0, false, false, false);
        assert!(kerned < ApproxMeasurer.text_width("AVATAR Toy To.", 16.0, false, false, false) - 9.0);
        let before = m.identity();
        if let Ok(bytes) = std::fs::read("C:/Users/nexga/Documents/wpt-corpus/fonts/Ahem.ttf") {
            store.register(bytes, "Ahem");
            assert_eq!(m.font_ascent_family(13.0, Some("Ahem")), 13.0 * 0.8);
            assert_eq!(m.text_width_family("XX", 15.0, Some("Ahem"), false, false, false), 30.0);
            assert_ne!(m.identity(), before);
        }
    }
}
