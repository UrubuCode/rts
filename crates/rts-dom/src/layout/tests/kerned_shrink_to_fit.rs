//! A shrink-to-fit box holds its text on ONE line when the font kerns across
//! a space. Its width is the max-content width, measured as one string
//! (`measure/text.rs`); the line breaker measured the same line word by word,
//! and with a kerning pair that spans the space (" T" in Times New Roman) the
//! pieces summed wider than the whole — so the box wrapped its own last word
//! (`AVATAR Toy To.` at 16px: 112.22 whole, 112.80 summed, 36px tall).

use super::*;

/// 8px a character, 2px narrower for every "AV" and every " T" — a synthetic
/// kerning table. Only the cross-space pair can tell a whole-string width
/// from a summed one; "AV" sits inside a word and is kerned either way.
struct Kerning;

impl Kerning {
    fn width(text: &str) -> f32 {
        text.chars().count() as f32 * 8.0 - 2.0 * (text.matches("AV").count() + text.matches(" T").count()) as f32
    }
}

impl TextMeasurer for Kerning {
    fn text_width(&self, text: &str, _: f32, _: bool, _: bool, _: bool) -> f32 {
        Kerning::width(text)
    }
    fn text_width_family(&self, text: &str, size: f32, _: Option<&str>, mono: bool, bold: bool, italic: bool) -> f32 {
        self.text_width(text, size, mono, bold, italic)
    }
    fn line_height(&self, size: f32) -> f32 {
        ApproxMeasurer.line_height(size)
    }
}

fn text_items(html: &str) -> Vec<(String, f32)> {
    let dom = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
    let m = Kerning;
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &m };
    let list = layout_document(&dom, &ctx);
    list.materialized()
        .iter()
        .filter_map(|it| match it {
            DisplayItem::Text { text, y, .. } => Some((text.to_string(), *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_inline_block_kerned_across_a_space_is_one_line_tall() {
    let items = text_items(r#"<div><span style="display:inline-block;font-size:16px">AVATAR Toy To.</span></div>"#);
    let ys: Vec<f32> = items.iter().map(|(_, y)| *y).collect();
    assert!(ys.windows(2).all(|w| w[0] == w[1]), "the text wrapped: {items:?}");
    let text: String = items.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(text.trim(), "AVATAR Toy To.", "items: {items:?}");
}

/// The re-measure only rescues a line the WHOLE width fits: a box narrower
/// than the kerned string still wraps where it did.
#[test]
fn a_box_narrower_than_the_kerned_line_still_wraps() {
    let whole = Kerning::width("AVATAR Toy To.");
    let items = text_items(&format!(r#"<div style="width:{}px;font-size:16px">AVATAR Toy To.</div>"#, whole - 1.0));
    let lines = items.iter().map(|(_, y)| y.to_bits()).collect::<std::collections::BTreeSet<_>>().len();
    assert_eq!(lines, 2, "items: {items:?}");
}
