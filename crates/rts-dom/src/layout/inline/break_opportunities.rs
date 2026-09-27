//! Where a WORD may break: the soft wrap opportunities inside text that has no
//! CSS white space in it (UAX #14 — after a hyphen, between ideographs, after
//! a zero-width space), and the grapheme clusters an emergency break cuts at.
//!
//! **The measurer answers, this module decides** (CSS Text 3 §5.1). The two
//! Unicode questions go to `TextMeasurer::line_break_opportunities` and
//! `TextMeasurer::grapheme_boundaries`, whose defaults answer what this engine
//! did before a UAX #14 table existed — no opportunity inside a word, a cut at
//! any `char` — so `ApproxMeasurer` lays out unchanged. What CSS does ON TOP is
//! here, in one place for the breaker (`line_break.rs`) and the min-content
//! width (`measure/text.rs`):
//!
//! - a soft hyphen is `hyphen.rs`'s (it paints a "-"), so its position is dropped;
//! - `white-space: nowrap`/`pre` take none (the caller passes `wraps`);
//! - `word-break: keep-all` drops an opportunity between two letters, which is
//!   what separates ideographs; the one after a hyphen or a ZWSP stays;
//! - `line-break: loose/normal/strict` are ONE rule, UAX #14's default: no test
//!   of this engine's rulers separates them (kinsoku is not implemented);
//! - `word-break: break-all`, `line-break: anywhere` and `overflow-wrap` are
//!   emergency breaks (`BreakWithin`), which cut at [`cluster_ends`].
//!
//! **Opportunities between two RUNS are not asked** (`<span>中</span><span>文</span>`):
//! the breaker glues runs with no white space into one cluster, as before.
//!
//! **The ASCII fast path.** A word made only of letters, digits and characters
//! of UAX #14 classes AL/NU/IS/QU has no opportunity inside it under any rule,
//! so it is answered without calling the measurer — no allocation, no
//! segmentation. That is almost every word of a Latin page; the test
//! `an_ascii_page_never_asks_the_measurer` counts the calls.

use super::*;

/// ASCII bytes whose UAX #14 class is AL, NU, IS or QU. No pair of those
/// classes has a break opportunity between them (LB13, LB19, LB23, LB25,
/// LB28, LB29), so a word made only of them never needs the table.
/// Deliberately left out: `- / ? ! ) ] } |` (a break may follow them) and
/// `( [ { $ % + \` (PR/PO/OP, whose pairs are rule-by-rule).
fn quiet_ascii(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b".,:;'\"#&*@=<>^_`~".contains(&b)
}

/// Byte offsets strictly inside `word` (no CSS white space in it) where a
/// line may break, with the CSS filters of the module doc applied.
pub(crate) fn inside_word(m: &dyn TextMeasurer, word: &str, keep_all: bool) -> Vec<usize> {
    if word.bytes().all(quiet_ascii) {
        return Vec::new();
    }
    let letter = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    m.line_break_opportunities(word)
        .into_iter()
        .filter(|&i| i > 0 && i < word.len())
        .filter(|&i| !word[..i].ends_with(super::hyphen::SHY))
        .filter(|&i| !(keep_all && letter(word[..i].chars().next_back()) && letter(word[i..].chars().next())))
        .collect()
}

/// `word` cut at [`inside_word`]'s offsets — one piece when it has none, or
/// when `wraps` is false (`nowrap`, `pre`: no soft wrap opportunity at all).
pub(crate) fn pieces<'a>(m: &dyn TextMeasurer, word: &'a str, wraps: bool, keep_all: bool) -> Pieces<'a> {
    let cuts = if wraps { inside_word(m, word, keep_all) } else { Vec::new() };
    Pieces { word, cuts: cuts.into_iter(), from: 0, done: false }
}

pub(crate) struct Pieces<'a> {
    word: &'a str,
    cuts: std::vec::IntoIter<usize>,
    from: usize,
    done: bool,
}

impl<'a> Iterator for Pieces<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let to = self.cuts.next().unwrap_or_else(|| {
            self.done = true;
            self.word.len()
        });
        let piece = &self.word[self.from..to];
        self.from = to;
        Some(piece)
    }
}

/// The byte offsets where each grapheme cluster of `text` ENDS — every place an
/// emergency break may cut, `text.len()` included, `0` not. ASCII answers its
/// byte offsets without asking (an ASCII cluster is one byte, bar CR LF, which
/// is white space and never inside a word).
pub(crate) fn cluster_ends(m: &dyn TextMeasurer, text: &str) -> Vec<usize> {
    if text.is_ascii() {
        return (1..=text.len()).collect();
    }
    m.grapheme_boundaries(text).into_iter().filter(|&i| i > 0).collect()
}

/// The length of `text`'s first grapheme cluster — what a line too narrow for
/// any glyph still takes, so the break loop advances.
pub(crate) fn first_cluster(m: &dyn TextMeasurer, text: &str) -> usize {
    match text.as_bytes().first() {
        None => 0,
        Some(b) if b.is_ascii() && text.as_bytes().get(1).is_none_or(u8::is_ascii) => 1,
        _ => cluster_ends(m, text).first().copied().unwrap_or(text.len()),
    }
}
