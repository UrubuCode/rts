//! `Intl.Segmenter` — where a language says one grapheme, word or sentence ends
//! and the next begins.
//!
//! Not `split(" ")`. Thai writes sentences with no spaces at all, an emoji with
//! a skin-tone modifier is one grapheme made of several code points, and
//! `"Mr. Smith"` is one sentence. Every boundary here is CLDR's and ICU4X's.

use icu::segmenter::options::{SentenceBreakInvariantOptions, WordBreakInvariantOptions};
use icu::segmenter::{GraphemeClusterSegmenter, SentenceSegmenter, WordSegmenter};

use super::super::{Context, with_current};
use crate::value::Value;

/// `Intl.Segmenter`.
#[rtse::class("Segmenter")]
impl Segmenter {
    /// `new Intl.Segmenter(locales, options)`.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let granularity =
            super::option(options, "granularity").unwrap_or_else(|| "grapheme".to_owned());
        let locale_text = selected.to_string();
        with_current(|context| {
            let locale_value = context
                .intern_value(crate::text::Str::from_str(&locale_text))
                .bits();
            let granularity_value = context
                .intern_value(crate::text::Str::from_str(&granularity))
                .bits();
            super::resolved(
                context,
                this,
                "Segmenter",
                &[
                    ("locale", locale_value),
                    ("granularity", granularity_value),
                ],
            )
        })
    }

    /// `seg.segment(text)` — the pieces, each with where it started.
    ///
    /// The language answers a `Segments` object that is iterable and can be
    /// asked about a position; this answers the ARRAY of the same records, for
    /// the reason [`super::super::iterate`] gives — `for`-`of` and `Array.from`
    /// both accept one. The divergence, named: `containing()` does not exist,
    /// and the pieces are all found before the first is read.
    fn segment(this: u64, text: u64) -> u64 {
        let Some(subject) = with_current(|context| {
            let cell = Value(text).as_slot()?;
            context.text_at(cell)?.to_rust()
        }) else {
            return with_current(|context| super::super::objects::undefined_of(context));
        };
        let granularity = super::stored_text(this, "granularity").unwrap_or_default();
        let pieces = pieces_of(&granularity, &subject);
        let values = with_current(|context| {
            pieces
                .into_iter()
                .filter_map(|(from, to, word_like)| {
                    record(context, &subject, from, to, word_like, text)
                })
                .collect::<Vec<u64>>()
        });
        with_current(|context| super::super::array::built_in(context, values))
    }

    /// `seg.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// One `{ segment, index, input, isWordLike }` record.
///
/// `index` counts UTF-16 units, not bytes: it is an index a program will use to
/// slice the string it passed in, and this crate's strings are units.
fn record(
    context: &mut Context,
    subject: &str,
    from: usize,
    to: usize,
    word_like: Option<bool>,
    input: u64,
) -> Option<u64> {
    let piece = subject.get(from..to)?;
    let cell = super::super::native::plain(context)?;
    let text = context
        .intern_value(crate::text::Str::from_str(piece))
        .bits();
    let key = context.well_known("segment");
    super::super::objects::put(context, cell, key, text);
    let at = subject[..from].encode_utf16().count() as f64;
    let key = context.well_known("index");
    super::super::objects::put(context, cell, key, Value::from_f64(at).bits());
    let key = context.well_known("input");
    super::super::objects::put(context, cell, key, input);
    // Only a word segmentation has this field, which is the specification's own
    // shape: asking whether a grapheme is "word-like" is not a question.
    if let Some(word_like) = word_like {
        let key = context.well_known("isWordLike");
        super::super::objects::put(context, cell, key, Value::from_bool(word_like).bits());
    }
    Some(Value::from_slot(cell).bits())
}

/// The boundaries a granularity produces, as byte ranges.
fn pieces_of(
    granularity: &str,
    subject: &str,
) -> Vec<(usize, usize, Option<bool>)> {
    match granularity {
        "word" => {
            // `auto` is what picks a dictionary or an LSTM for the scripts that
            // write without spaces — Thai, Khmer, Lao — and it is why the locale
            // is not passed: the boundary rules are invariant, and the choice
            // between engines is made from the TEXT rather than from the tag.
            let found = WordSegmenter::new_auto(WordBreakInvariantOptions::default())
                .segment_str(subject);
            let mut pieces = Vec::new();
            let mut previous = 0;
            // `iter_with_word_type` is what carries the one field a word
            // segmentation has that the others do not, so the boundaries and
            // the flag come from ONE walk rather than two that could disagree.
            for boundary in found.iter_with_word_type().skip(1) {
                pieces.push((previous, boundary.0, Some(boundary.1.is_word_like())));
                previous = boundary.0;
            }
            pieces
        }
        "sentence" => ranges(
            SentenceSegmenter::new(SentenceBreakInvariantOptions::default())
                .segment_str(subject)
                .collect(),
        ),
        _ => ranges(
            GraphemeClusterSegmenter::new()
                .segment_str(subject)
                .collect(),
        ),
    }
}

/// Boundaries into ranges, dropping the leading zero the segmenters answer.
fn ranges(boundaries: Vec<usize>) -> Vec<(usize, usize, Option<bool>)> {
    boundaries
        .windows(2)
        .map(|pair| (pair[0], pair[1], None))
        .collect()
}
