//! The breaker asks the MEASURER where a word may break and where a character
//! ends (`inline/break_opportunities.rs`), and applies `white-space` and
//! `word-break` on top. A recording measurer answers a small, fixed subset of
//! UAX #14/#29 — enough to tell each rule apart — and counts the questions.

use super::*;
use std::cell::Cell;

/// 10px a `char`, 0 for U+0301 and U+200D. Opportunities: after `-`, and
/// between two ideographs (U+4E00–U+9FFF). Clusters: a U+0301 or a U+200D
/// joins the char before it, and the char after a U+200D joins it too.
#[derive(Default)]
struct Recording {
    opportunity_calls: Cell<usize>,
    cluster_calls: Cell<usize>,
}

fn ideograph(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
}

impl TextMeasurer for Recording {
    fn text_width(&self, text: &str, _: f32, _: bool, _: bool, _: bool) -> f32 {
        text.chars().filter(|c| !matches!(c, '\u{301}' | '\u{200D}')).count() as f32 * 10.0
    }
    fn line_height(&self, _: f32) -> f32 {
        20.0
    }
    fn line_break_opportunities(&self, text: &str) -> Vec<usize> {
        self.opportunity_calls.set(self.opportunity_calls.get() + 1);
        let v: Vec<(usize, char)> = text.char_indices().collect();
        v.windows(2).filter(|w| w[0].1 == '-' || (ideograph(w[0].1) && ideograph(w[1].1))).map(|w| w[1].0).collect()
    }
    fn grapheme_boundaries(&self, text: &str) -> Vec<usize> {
        self.cluster_calls.set(self.cluster_calls.get() + 1);
        let mut out = vec![0];
        let mut prev = None;
        for (i, c) in text.char_indices().skip(1) {
            if !matches!(c, '\u{301}' | '\u{200D}') && prev != Some('\u{200D}') {
                out.push(i);
            }
            prev = Some(c);
        }
        out.push(text.len());
        out.dedup();
        out
    }
}

/// The text of each line, in order, and the measurer that laid it out.
fn lines(body: &str) -> (Vec<String>, Recording) {
    let dom = parse_html_to_dom(&format!("<style>body{{margin:0;font-size:10px}}</style>{body}"));
    let m = Recording::default();
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &m };
    let list = layout_document(&dom, &ctx);
    let mut rows: std::collections::BTreeMap<u32, String> = Default::default();
    for it in list.materialized().iter() {
        if let DisplayItem::Text { text, y, .. } = it {
            rows.entry(y.to_bits()).or_default().push_str(text);
        }
    }
    let mut ys: Vec<(u32, String)> = rows.into_iter().collect();
    ys.sort_by(|a, b| f32::from_bits(a.0).total_cmp(&f32::from_bits(b.0)));
    (ys.into_iter().map(|(_, t)| t.trim().to_string()).collect(), m)
}

#[test]
fn ideographs_break_between_each_other_without_a_space() {
    let (l, _) = lines("<div style='width:30px'>一二三四五六</div>");
    assert_eq!(l, ["一二三", "四五六"]);
}

#[test]
fn keep_all_drops_the_opportunity_between_ideographs() {
    let (l, _) = lines("<div style='width:30px;word-break:keep-all'>一二三四五六</div>");
    assert_eq!(l, ["一二三四五六"]);
}

#[test]
fn a_hard_hyphen_is_an_opportunity_and_nowrap_takes_none() {
    let (l, _) = lines("<div style='width:50px'>abc-defg</div>");
    assert_eq!(l, ["abc-", "defg"]);
    let (l, _) = lines("<div style='width:50px;white-space:nowrap'>abc-defg</div>");
    assert_eq!(l, ["abc-defg"]);
}

/// U+00A0 is not white space to CSS and not an opportunity to UAX #14: the
/// two words overflow together rather than break.
#[test]
fn a_no_break_space_does_not_break() {
    let (l, _) = lines("<div style='width:30px'>abcd\u{a0}efgh</div>");
    assert_eq!(l, ["abcd\u{a0}efgh"]);
}

/// `break-all` cuts where a CLUSTER ends: `e` + U+0301 stays whole.
#[test]
fn break_all_cuts_at_a_grapheme_boundary() {
    let (l, m) = lines("<div style='width:20px;word-break:break-all'>xe\u{301}yz</div>");
    assert_eq!(l, ["xe\u{301}", "yz"]);
    assert!(m.cluster_calls.get() > 0);
}

/// `overflow-wrap: anywhere` keeps a ZWJ sequence on one line.
#[test]
fn an_emergency_break_never_splits_a_zwj_sequence() {
    let family = "\u{1F468}\u{200D}\u{1F469}";
    let (l, _) = lines(&format!("<div style='width:10px;overflow-wrap:anywhere'>a{family}b</div>"));
    assert_eq!(l, ["a".to_string(), family.to_string(), "b".to_string()]);
}

/// The ASCII fast path: a page of plain Latin words never asks the measurer
/// for opportunities or clusters, even where lines wrap.
#[test]
fn an_ascii_page_never_asks_the_measurer() {
    let text = "The quick brown fox, jumping; over the lazy dog's back: 42 times. ".repeat(20);
    let (l, m) = lines(&format!("<div style='width:200px'>{text}</div><div style='width:200px;word-break:break-all'>{text}</div>"));
    assert!(l.len() > 20);
    assert_eq!((m.opportunity_calls.get(), m.cluster_calls.get()), (0, 0));
}

/// Min-content takes the same pieces as the breaker: a shrink-to-fit box
/// around `abc-defg` is as wide as `defg`, not the whole word.
#[test]
fn min_content_breaks_at_the_same_opportunities() {
    let dom = parse_html_to_dom("<style>body{margin:0;font-size:10px}</style><div style='width:0'><span id=s style='display:inline-block'>abc-defg 一二</span></div>");
    let m = Recording::default();
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &m };
    let list = layout_document(&dom, &ctx);
    let id = dom.resolve(dom.query("#s").expect("#s")).expect("a live node");
    assert_eq!(list.rect_of(id).map(|r| r.w), Some(40.0));
}
