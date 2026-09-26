//! One loaded font face: its bytes, its identity and the vertical metrics
//! Blink lays a line out with.
//!
//! The metrics reproduce what `rts-dom`'s `layout/measure/font_metrics.rs`
//! documents as measured in Edge 153: ascent and descent are each rounded to a
//! whole pixel ON THEIR OWN, and the `normal` line height is
//! `round(round(ascent) + round(descent) + gap)`. The unrounded sources are
//! what DirectWrite gives Blink — see [`vertical_metrics`]. None of the four
//! Windows defaults sets `USE_TYPO_METRICS`, so the 28-row table pins the
//! win/hhea arm; the typo arm is untested by that table and says so here
//! rather than in a test that cannot exist yet.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use rustybuzz::ttf_parser;

/// A parsed, immutable face. Cheap to clone through `Arc`.
pub struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
    id: u64,
    family: String,
    weight: u16,
    italic: bool,
    upem: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
}

impl Face {
    /// Parses face `index` of `data`. `family` overrides the `name` table's
    /// family — what a host registering bytes under a CSS name wants; `None`
    /// reads it from the font. `None` back when the bytes are not a font.
    pub fn parse(data: Arc<Vec<u8>>, index: u32, family: Option<&str>) -> Option<Face> {
        let f = ttf_parser::Face::parse(&data, index).ok()?;
        let upem = f.units_per_em();
        let (ascender, descender, line_gap) = vertical_metrics(&f);
        let family = match family {
            Some(name) => name.to_owned(),
            None => family_name(&f)?,
        };
        let weight = f.tables().os2.map_or(400, |o| o.weight().to_number());
        let italic = f.is_italic() || f.is_oblique();
        let mut h = DefaultHasher::new();
        data.hash(&mut h);
        index.hash(&mut h);
        family.to_ascii_lowercase().hash(&mut h);
        let id = h.finish();
        Some(Face { data, index, id, family, weight, italic, upem, ascender, descender, line_gap })
    }

    /// The font's bytes and face index, for the shaper and the rasteriser.
    pub fn data(&self) -> (&[u8], u32) {
        (&self.data, self.index)
    }

    /// Stable identity: a hash of the bytes, the index and the family it was
    /// registered under. Two faces with one id answer the same numbers.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn family(&self) -> &str {
        &self.family
    }

    /// `OS/2` usWeightClass (400 when the table is absent).
    pub fn weight(&self) -> u16 {
        self.weight
    }

    pub fn italic(&self) -> bool {
        self.italic
    }

    pub fn units_per_em(&self) -> u16 {
        self.upem
    }

    /// The factor from font units to pixels at `size` px.
    pub fn units_to_px(&self, size: f32) -> f32 {
        size / f32::from(self.upem)
    }

    /// Ascent in pixels, rounded to a whole pixel as Blink rounds it.
    /// The arithmetic is `size × (units / upem)` in `f32`, the same order as
    /// the generated table, so a half lands on the same side.
    pub fn ascent_px(&self, size: f32) -> f32 {
        (size * (f32::from(self.ascender) / f32::from(self.upem))).round()
    }

    /// Descent in pixels (positive), rounded on its own — not `line − ascent`.
    pub fn descent_px(&self, size: f32) -> f32 {
        (size * (-f32::from(self.descender) / f32::from(self.upem))).round()
    }

    /// The unrounded line gap in pixels.
    pub fn line_gap_px(&self, size: f32) -> f32 {
        size * (f32::from(self.line_gap) / f32::from(self.upem))
    }

    /// The height of one line under `line-height: normal`.
    pub fn normal_line_height(&self, size: f32) -> f32 {
        (self.ascent_px(size) + self.descent_px(size) + self.line_gap_px(size)).round()
    }

    /// The `hmtx` advance of `c`'s nominal glyph, in font units — no
    /// shaping, no kerning. `None` when the font has no glyph for it.
    pub fn nominal_advance(&self, c: char) -> Option<u16> {
        let f = ttf_parser::Face::parse(&self.data, self.index).ok()?;
        f.glyph_hor_advance(f.glyph_index(c)?)
    }
}

/// Ascender, descender (negative) and line gap in font units, as DirectWrite
/// answers them to Blink on Windows:
/// - `USE_TYPO_METRICS` set: the `OS/2` typographic triple;
/// - otherwise `usWinAscent`/`usWinDescent`, and a line gap of
///   `max(0, hhea extent − win extent)` where each extent is ascender −
///   descender (+ the hhea gap).
///
/// Not the `hhea` triple, which is what `font_metrics.rs`'s header says: that
/// is indistinguishable for Times, Arial and Segoe UI (their win and hhea
/// values coincide) and wrong for Consolas, whose hhea is 1521/−527/350
/// against win 1884/514 — the measured row (15/4 at 16px) is win's, and
/// the gap 0 is 1521 + 527 + 350 − 1884 − 514. Measured on this machine's
/// files while writing this; `tests/blink_tables.rs` is the ruler.
/// Without an `OS/2` table the `hhea` triple stands in.
fn vertical_metrics(f: &ttf_parser::Face) -> (i16, i16, i16) {
    let hhea = f.tables().hhea;
    let Some(os2) = f.tables().os2 else { return (hhea.ascender, hhea.descender, hhea.line_gap) };
    if os2.use_typographic_metrics() {
        return (os2.typographic_ascender(), os2.typographic_descender(), os2.typographic_line_gap());
    }
    let (asc, desc) = (os2.windows_ascender(), os2.windows_descender());
    let hhea_extent = i32::from(hhea.ascender) - i32::from(hhea.descender) + i32::from(hhea.line_gap);
    let win_extent = i32::from(asc) - i32::from(desc);
    let gap = (hhea_extent - win_extent).clamp(0, i32::from(i16::MAX)) as i16;
    (asc, desc, gap)
}

/// The family name a CSS `font-family` matches: the typographic family
/// (name id 16) when present, else the legacy family (id 1). English (or any
/// Unicode) record first, since Windows files carry localised names too.
pub(crate) fn family_name(f: &ttf_parser::Face) -> Option<String> {
    use ttf_parser::name_id::{FAMILY, TYPOGRAPHIC_FAMILY};
    for id in [TYPOGRAPHIC_FAMILY, FAMILY] {
        let mut fallback = None;
        for n in f.names().into_iter().filter(|n| n.name_id == id) {
            let Some(s) = n.to_string() else { continue };
            if n.language_id == 0x0409 {
                return Some(s);
            }
            fallback.get_or_insert(s);
        }
        if fallback.is_some() {
            return fallback;
        }
    }
    None
}
