//! The window's text: measured by `rts_text::adapter::RealMeasurer`, painted
//! by epaint with the SAME face (plan `2026-09-26-text-crate.md`, F4).
//!
//! This replaced `EguiMeasurer`, which measured with epaint's glyph advances
//! (no kerning), took the ascent from epaint's `StyledMetrics` and had no
//! descent at all (it fell back to the trait's `0.3125 × size`). It also
//! measured unstyled text in Segoe UI while `claude-raster` and Blink use
//! Times New Roman, so the window and the instrument disagreed on the same
//! page. `RealMeasurer` is the one implementation both now share.
//!
//! Painting stays on epaint's galley in this lot. What must not diverge is
//! the FACE: `painted_family` asks the measurer which face decided the width
//! and hands those very bytes to egui under a name derived from the face's
//! id. Choosing the egui font by a second rule (the old bold/italic table
//! over `app/fonts.rs`'s hard-coded paths) is the two-answers class this
//! module exists to close.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use rts_text::adapter::RealMeasurer;
use rts_text::FontStore;

thread_local! {
    /// One measurer per UI thread, i.e. per app: its `FontStore` loads each
    /// system face once and its `identity()` is stable across frames, so the
    /// layout cache keyed by it hits frame after frame.
    static MEASURER: Rc<RealMeasurer> = Rc::new(RealMeasurer::new(Arc::new(FontStore::new())));
    /// egui font names already handed to `add_font`. egui installs a font at
    /// the start of the NEXT pass and only dedups against installed ones, so
    /// without this every frame until then would copy the face's bytes again.
    static REQUESTED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// The app's measurer, registered as the thread's ACTIVE one so the geometry
/// answered outside the frame (`getBoundingClientRect`, `computedProperty`)
/// is the geometry this frame paints. Re-registered every frame because
/// "last to paint wins" is the contract of `active_measurer`; it costs an
/// `Rc` clone.
pub(super) fn measurer() -> Rc<RealMeasurer> {
    let measurer = MEASURER.with(Rc::clone);
    rts_dom::layout::active_measurer::set_active(measurer.clone());
    measurer
}

/// The egui family to paint a `DisplayItem::Text` with: the face the
/// measurer resolved for `(family, mono, bold, italic)`, registered in egui
/// from its own bytes on first sight.
///
/// Until egui has installed it (the next pass) — and on a machine where the
/// family resolves to no face, where the measurer answers `ApproxMeasurer`'s
/// numbers — the text is painted with `legacy_family`, the fonts
/// `app/fonts.rs` loads at start-up. The first case lasts one frame and
/// requests a repaint.
pub(super) fn painted_family(
    ctx: &egui::Context,
    family: Option<&str>,
    mono: bool,
    bold: bool,
    italic: bool,
) -> egui::FontFamily {
    let Some(face) = MEASURER.with(|m| m.face(family, mono, bold, italic)) else {
        return legacy_family(mono, bold, italic);
    };
    let name = format!("rts-text:{:016x}", face.id());
    let egui_family = egui::FontFamily::Name(name.as_str().into());
    if ctx.fonts(|f| f.definitions().families.contains_key(&egui_family)) {
        return egui_family;
    }
    if REQUESTED.with(|r| r.borrow_mut().insert(name.clone())) {
        let (bytes, index) = face.data();
        let data = egui::FontData { index, ..egui::FontData::from_owned(bytes.to_vec()) };
        let insert = egui::epaint::text::InsertFontFamily {
            family: egui_family,
            priority: egui::epaint::text::FontPriority::Highest,
        };
        ctx.add_font(egui::epaint::text::FontInsert::new(&name, data, vec![insert]));
        ctx.request_repaint();
    }
    legacy_family(mono, bold, italic)
}

/// The families `app/fonts.rs` always installs. Weight and style are two
/// axes: `<em><strong>` asks for "bold-italic", its own file.
fn legacy_family(mono: bool, bold: bool, italic: bool) -> egui::FontFamily {
    match (bold, italic) {
        (true, true) => egui::FontFamily::Name("bold-italic".into()),
        (true, false) => egui::FontFamily::Name("bold".into()),
        (false, true) => egui::FontFamily::Name("italic".into()),
        (false, false) if mono => egui::FontFamily::Monospace,
        (false, false) => egui::FontFamily::Proportional,
    }
}
