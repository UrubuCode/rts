//! EVERY INLINE BOX HAS ITS OWN FONT (CSS 2.1 §10.8).
//!
//! The inline flow measured and painted all the text of a line with the
//! CONTAINER's family and size: a `<code>` inside a serif paragraph was
//! measured in Times, a 32px `<span>` at 16px, and the line box never grew
//! for a bigger inline (`claude-fonte-por-trecho`, measured in Blink). The
//! defect hid behind the old text measurer, which had one average advance for
//! every proportional font — all families measured alike, so nobody could see
//! the wrong one being asked.
//!
//! A run does not need a new field to know its font: it already carries its
//! `owners`, the inline elements around it, and the INNERMOST one is the
//! element whose computed style is the text's — size, family, line height.
//! This module is that lookup, in the two shapes the flow needs: per RUN while
//! breaking lines ([`Fonts`]), per SEGMENT while placing them
//! ([`of_segment`]). A run whose font is the container's answers `None` on
//! both, and nothing changes for it — which is every run of most lines.

use super::*;

/// The font of one run or segment, where it differs from the container's.
pub(in crate::layout) struct Font {
    pub(in crate::layout) size: f32,
    pub(in crate::layout) family: Option<String>,
    pub(in crate::layout) mono: bool,
}

impl Font {
    fn ahem(&self) -> bool {
        crate::layout::measure::font_metrics::uses_ahem(self.family.as_deref())
    }
}

/// The font of the innermost owner, or `None` when it is the container's own
/// (same size AND same family list) or there is no owner.
fn of_owner(dom: &Dom, owners: &[NodeIdx], base_family: Option<&str>, base_size: f32) -> Option<(Font, std::rc::Rc<ComputedStyle>)> {
    let css = dom.computed_style_idx(*owners.last()?)?;
    let size = font_px(&css, base_size);
    let family = css.font_family.clone();
    if (size - base_size).abs() < 0.001 && family.as_deref() == base_family {
        return None;
    }
    let mono = family.as_deref().is_some_and(crate::style::is_mono_family);
    Some((Font { size, family, mono }, css))
}

/// The fonts of a flow's runs, for line breaking: the container's, and each
/// run's own where it differs.
pub(in crate::layout) struct Fonts<'a> {
    base_family: Option<&'a str>,
    base_size: f32,
    base_mono: bool,
    per_run: Vec<Option<Font>>,
}

impl<'a> Fonts<'a> {
    pub(in crate::layout) fn of_flow(dom: &Dom, runs: &[InlineRun], family: Option<&'a str>, size: f32, mono: bool) -> Self {
        let per_run = runs
            .iter()
            .map(|r| if r.atomic.is_some() { None } else { of_owner(dom, &r.owners, family, size).map(|(f, _)| f) })
            .collect();
        Fonts { base_family: family, base_size: size, base_mono: mono, per_run }
    }

    /// A flow of one font — the generated box's own text.
    pub(in crate::layout) fn uniform(family: Option<&'a str>, size: f32, mono: bool) -> Self {
        Fonts { base_family: family, base_size: size, base_mono: mono, per_run: Vec::new() }
    }

    pub(in crate::layout) fn base_ahem(&self) -> bool {
        crate::layout::measure::font_metrics::uses_ahem(self.base_family)
    }

    /// The width of `t` in the font of run `i` (any index past the runs — the
    /// container's own space — is the base font).
    pub(in crate::layout) fn width(&self, m: &dyn TextMeasurer, i: usize, t: &str, bold: bool, italic: bool) -> f32 {
        match self.per_run.get(i).and_then(Option::as_ref) {
            Some(f) if f.ahem() => t.chars().count() as f32 * f.size,
            Some(f) => m.text_width_family(t, f.size, f.family.as_deref(), f.mono, bold, italic),
            None if self.base_ahem() => t.chars().count() as f32 * self.base_size,
            None => m.text_width_family(t, self.base_size, self.base_family, self.base_mono, bold, italic),
        }
    }
}

/// How a segment of text with its OWN font sits on the line: what to paint it
/// with, and the inline box it contributes to the line's envelope.
pub(in crate::layout) struct SegmentFont {
    pub(in crate::layout) font: Font,
    pub(in crate::layout) ahem: bool,
    /// From the top of the glyphs' content area to the baseline.
    pub(in crate::layout) ascent: f32,
    /// The inline box: its height is the owner's `line-height`, and its top is
    /// `box_ascent` above the baseline (half-leading + ascent) — the pair
    /// `vertical_align::envelope_com_baseline` takes.
    pub(in crate::layout) box_height: f32,
    pub(in crate::layout) box_ascent: f32,
}

pub(in crate::layout) fn of_segment(
    dom: &Dom,
    owners: &[NodeIdx],
    base_family: Option<&str>,
    base_size: f32,
    m: &dyn TextMeasurer,
) -> Option<SegmentFont> {
    let (font, css) = of_owner(dom, owners, base_family, base_size)?;
    let font_family = font.family.as_deref();
    let ascent = m.font_ascent_family(font.size, font_family);
    let content = crate::inline_box::altura_do_conteudo(font.size, font_family, m);
    let box_height = crate::inline_box::altura_da_linha(&css, font.size, m);
    let box_ascent = crate::inline_box::meia_entrelinha(box_height, content) + ascent;
    Some(SegmentFont { ahem: font.ahem(), font, ascent, box_height, box_ascent })
}
