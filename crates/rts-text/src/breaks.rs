//! Where a line MAY break (UAX #14) and where a character ends (UAX #29
//! extended grapheme clusters), as byte indices of the run's text. Which of
//! them the inline layout takes — `white-space`, `word-break`,
//! `overflow-wrap` — is `rts-dom`'s decision, not this module's.

use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

/// Byte indices at which a line may end, allowed and mandatory alike, in
/// increasing order. The end of the text, which UAX #14 always reports as a
/// mandatory break, is left out: it is where the run ends, not a choice.
pub fn line_break_opportunities(text: &str) -> Vec<usize> {
    linebreaks(text)
        .filter(|&(i, op)| !(i == text.len() && op == BreakOpportunity::Mandatory))
        .map(|(i, _)| i)
        .collect()
}

/// Byte indices of every grapheme cluster boundary, `0` and `text.len()`
/// included, so consecutive pairs are the clusters.
pub fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut out: Vec<usize> = text.grapheme_indices(true).map(|(i, _)| i).collect();
    out.push(text.len());
    out
}
