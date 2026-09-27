//! Text in `claude-raster`: laid out with `rts_text`'s `RealMeasurer` and
//! painted from the same faces — each glyph's coverage composited over the
//! canvas (plan `docs/superpowers/plans/2026-09-26-text-crate.md`, F3).
//!
//! # Which face paints an item
//!
//! `DisplayItem::Text::family` is the computed `font-family` list the layout
//! measured the item with — the string its `text_width_family` received — so
//! the painter resolves that list through the same `FontStore` and draws the
//! face that decided the width. `None` means the layout measured with no
//! family, and it resolves as `RealMeasurer` does then: `monospace` when
//! `mono`, else `serif`. An item is masked only when no face on this machine
//! resolves; that is the one class the masked count reports.
//!
//! `is_ahem` needs no lookup: it is the family, by the same rule the layout
//! used (`style::is_ahem_family`).
//!
//! # Ahem is this same path
//!
//! `fill_ahem_text` (one solid square per non-space character) is gone: the
//! glyphs come from Ahem.ttf, shaped like any face. A glyph whose outline is
//! one solid rectangle — `X`, and most of Ahem — is filled with the canvas's
//! `fill_rect` at its unrounded position (`solid_rect` says why), which is
//! byte for byte what `fill_ahem_text` drew for a square. What differs is the
//! font's own doing: `p` is only its descender and `É` only its ascender, a
//! space is its own empty glyph, and a glyph that is not one rectangle is
//! composited from its coverage like any other face.
//!
//! What is still not drawn, as before: `decoration` (underline/line-through)
//! and anything a `transform` does to a glyph beyond moving its origin.

use std::collections::HashMap;
use std::sync::Arc;

use rts_dom::layout::{ApproxMeasurer, TextMeasurer};
use rts_dom::paint::Rect;
use rts_text::adapter::RealMeasurer;
use rts_text::{Bitmap, Face, FontStore, Style};

use crate::canvas::Canvas;

/// Where the WPT checkout keeps Ahem on the machine that owns this corpus;
/// `RTS_AHEM` (set by `scripts/wpt_reftests.mjs`) wins.
const DEFAULT_AHEM: &str = "C:/Users/nexga/Documents/wpt-corpus/fonts/Ahem.ttf";

/// A store over the system fonts, with Ahem registered when its file exists.
pub fn font_store() -> Arc<FontStore> {
    let store = Arc::new(FontStore::new());
    let path = std::env::var("RTS_AHEM").unwrap_or_else(|_| DEFAULT_AHEM.to_string());
    let registered = std::fs::read(&path).map(|bytes| store.register(bytes, "Ahem")).unwrap_or(0);
    if registered == 0 {
        eprintln!("rts-raster: no Ahem at {path}; Ahem text will be masked");
    }
    store
}

/// One `DisplayItem::Text`, already moved to canvas coordinates; `y` is the
/// TOP of its text box, as the item carries it.
pub struct TextRun<'a> {
    pub x: f32,
    pub y: f32,
    pub text: &'a str,
    pub size: f32,
    pub color: u32,
    pub mono: bool,
    pub is_ahem: bool,
    /// The family list the layout measured this item with.
    pub family: Option<&'a str>,
    pub bold: bool,
    pub italic: bool,
    pub letter_spacing: f32,
}

/// What happened to one item.
pub enum Outcome {
    Painted,
    /// No face on this machine resolves the item's family.
    NoFace(Rect),
}

pub struct TextPainter<'m> {
    measurer: &'m RealMeasurer,
    glyphs: HashMap<(u64, u16, u32), Option<Bitmap>>,
    rects: HashMap<(u64, u16), Option<[f32; 4]>>,
}

impl<'m> TextPainter<'m> {
    pub fn new(measurer: &'m RealMeasurer) -> TextPainter<'m> {
        TextPainter { measurer, glyphs: HashMap::new(), rects: HashMap::new() }
    }

    pub fn paint(&mut self, canvas: &mut Canvas, run: &TextRun, clip: Option<Rect>) -> Outcome {
        // `is_ahem` is the family by the layout's own rule
        // (`style::is_ahem_family`), and for a control it wins over the
        // `Arial` its label was measured in, as it does in the layout's widths.
        let family = if run.is_ahem { Some("Ahem") } else { run.family };
        let Some(face) = self.face(family, run) else {
            return Outcome::NoFace(self.mask(run));
        };
        // The baseline the layout placed: the item's top plus the ascent it
        // measured with — rts-dom's unrounded 0.8 em for Ahem (`RealMeasurer`
        // asks the same), the face's rounded ascent otherwise.
        let ascent = if run.is_ahem {
            run.size * rts_dom::style::AHEM_ASCENT_RATIO
        } else {
            self.measurer.font_ascent_family(run.size, family)
        };
        let baseline = run.y + ascent;
        let (r, g, b, a) = crate::canvas::argb_bytes(run.color);
        let rgb = (u32::from(r) << 24) | (u32::from(g) << 16) | (u32::from(b) << 8);
        let mut pen = run.x;
        for glyph in rts_text::shape(&face, run.text, run.size, true) {
            let (gx, gy) = (pen + glyph.x_offset, baseline - glyph.y_offset);
            match run.is_ahem.then(|| self.solid_rect(&face, glyph.id)).flatten() {
                Some([l, t, w, h]) => {
                    let s = run.size;
                    canvas.fill_rect(Rect::new(gx + l * s, gy - t * s, w * s, h * s), run.color, clip);
                }
                None => self.composite(canvas, &face, glyph.id, run.size, (gx, gy), (rgb, a), clip),
            }
            pen += glyph.x_advance + run.letter_spacing;
        }
        Outcome::Painted
    }

    /// Composites glyph `id`'s coverage with its origin at `(x, y)` (the
    /// baseline), rounded to the pixel grid — Blink snaps text to whole
    /// pixels on Windows, and every face but Ahem has a rounded ascent, so
    /// the baseline is whole already wherever the line top is.
    #[allow(clippy::too_many_arguments)]
    fn composite(&mut self, canvas: &mut Canvas, face: &Face, id: u16, size: f32, (x, y): (f32, f32), (rgb, a): (u32, u8), clip: Option<Rect>) {
        let key = (face.id(), id, size.to_bits());
        let Some(bm) = self.glyphs.entry(key).or_insert_with(|| rts_text::rasterise(face, id, size)) else { return };
        let ox = x.round() as i32 + bm.left;
        let oy = y.round() as i32 - bm.top;
        for row in 0..bm.h as i32 {
            for col in 0..bm.w as i32 {
                let cov = u32::from(bm.alpha[(row * bm.w as i32 + col) as usize]);
                // Colour × coverage × the colour's own alpha: `blend` mixes
                // straight RGB by that product, which is the premultiplied
                // `src·α + dst·(1−α)` over.
                let alpha = (cov * u32::from(a) + 127) / 255;
                if alpha > 0 {
                    canvas.blend(ox + col, oy + row, rgb | alpha, clip);
                }
            }
        }
    }

    /// When glyph `id` is ONE solid axis-aligned rectangle, that rectangle in
    /// em (`[left, top above the baseline, width, height]`), found by
    /// rasterising the glyph at one pixel per font unit; `None` for any other
    /// shape. Asked for Ahem only, and why: rts-dom gives Ahem UNROUNDED
    /// metrics (0.8/0.2 em), so its glyphs land on fractional pixels, and an
    /// Ahem reftest compares them against boxes, which this raster fills
    /// `floor..ceil`. Filling the glyph with the same `fill_rect` keeps a glyph
    /// and a box of one geometry the same pixels, as they are in Blink;
    /// coverage at a rounded baseline differed from the box by a row
    /// (`css-position/static-position/*`, measured).
    fn solid_rect(&mut self, face: &Face, id: u16) -> Option<[f32; 4]> {
        let upem = f32::from(face.units_per_em());
        *self.rects.entry((face.id(), id)).or_insert_with(|| {
            let bm = rts_text::rasterise(face, id, upem)?;
            if bm.w == 0 || bm.alpha.iter().any(|&c| c != 255) {
                return None;
            }
            Some([bm.left as f32 / upem, bm.top as f32 / upem, bm.w as f32 / upem, bm.h as f32 / upem])
        })
    }

    /// The face that measured this run: `RealMeasurer`'s own rule — no family
    /// means Blink's default, `monospace` when `mono`, else `serif`.
    fn face(&self, family: Option<&str>, run: &TextRun) -> Option<Arc<Face>> {
        let list = family.unwrap_or(if run.mono { "monospace" } else { "serif" });
        let style = if run.italic { Style::Italic } else { Style::Normal };
        self.measurer.store().resolve(list, if run.bold { 700 } else { 400 }, style)
    }

    /// The area an unpainted item covers, for the `.mask.json` the comparator
    /// excludes — the same rectangle the raster has always masked
    /// (`ApproxMeasurer`'s width, `size` above the top to 0.3 below), so a
    /// masked item masks exactly what it masked before this lot.
    fn mask(&self, run: &TextRun) -> Rect {
        let w = ApproxMeasurer.text_width(run.text, run.size, run.mono, false, false);
        Rect::new(run.x, run.y - run.size, w, run.size * 1.3)
    }
}
