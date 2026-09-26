//! `DisplayItem::Text::family` is the family the layout MEASURED the run with
//! — the painter resolves it, so it must be the very string
//! `text_width_family` received, or the painted face is not the measured one.

use super::*;
use std::cell::RefCell;

/// `ApproxMeasurer`, remembering every `(text, family)` it was asked to
/// measure through `text_width_family`.
struct Recorder(RefCell<Vec<(String, Option<String>)>>);

impl TextMeasurer for Recorder {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32 {
        ApproxMeasurer.text_width(text, size, mono, bold, italic)
    }
    fn text_width_family(&self, text: &str, size: f32, family: Option<&str>, mono: bool, bold: bool, italic: bool) -> f32 {
        self.0.borrow_mut().push((text.to_string(), family.map(str::to_string)));
        ApproxMeasurer.text_width_family(text, size, family, mono, bold, italic)
    }
    fn line_height(&self, size: f32) -> f32 {
        ApproxMeasurer.line_height(size)
    }
    fn line_height_family(&self, size: f32, family: Option<&str>) -> f32 {
        ApproxMeasurer.line_height_family(size, family)
    }
    fn font_ascent_family(&self, size: f32, family: Option<&str>) -> f32 {
        ApproxMeasurer.font_ascent_family(size, family)
    }
    fn font_descent_family(&self, size: f32, family: Option<&str>) -> f32 {
        ApproxMeasurer.font_descent_family(size, family)
    }
}

fn texts(html: &str) -> (Vec<(String, Option<String>)>, Vec<(String, Option<String>)>) {
    let dom = parse_html_to_dom(&format!("<style>body{{margin:0}}</style>{html}"));
    let rec = Recorder(RefCell::new(Vec::new()));
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &rec };
    let list = layout_document(&dom, &ctx);
    let items = list
        .materialized()
        .iter()
        .filter_map(|it| match it {
            DisplayItem::Text { text, family, .. } => Some((text.to_string(), family.as_deref().map(str::to_string))),
            _ => None,
        })
        .collect();
    (items, rec.0.into_inner())
}

#[test]
fn a_span_with_its_own_family_paints_with_the_family_it_was_measured_in() {
    let (items, asked) = texts(r#"<p style="font-family: Georgia">aaa <span style="font-family: Consolas">bbb</span></p>"#);
    let family_of = |t: &str| items.iter().find(|(s, _)| s.contains(t)).map(|(_, f)| f.clone());
    assert_eq!(family_of("bbb"), Some(Some("Consolas".to_string())), "items: {items:?}");
    assert_eq!(family_of("aaa"), Some(Some("Georgia".to_string())), "items: {items:?}");
    // One source: the family on the item is one the measurer received for that text.
    assert!(asked.iter().any(|(t, f)| t.contains("bbb") && f.as_deref() == Some("Consolas")), "asked: {asked:?}");
}
