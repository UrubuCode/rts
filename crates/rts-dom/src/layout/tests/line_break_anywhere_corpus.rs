//! `line-break: anywhere` (CSS Text 3 §5.1) as its own soft-wrap opportunity,
//! not routed through `word-break`/`overflow-wrap`.
//!
//! **Not a copy of WPT source.** Per `THIRD-PARTY-NOTICES.md`'s
//! web-platform-tests section, this engine copies nothing from the WPT
//! checkout — `tests/css/` and this corpus are our own fixtures, written
//! here. The scenario below is inspired by
//! `css/css-text/white-space/break-spaces-before-first-char-007.html` (WPT,
//! dual BSD-3-Clause/W3C Document Licence, not redistributed), which pairs
//! `line-break: anywhere` with `white-space: break-spaces` — a value this
//! tree does not have yet (added by a commit later than the one this
//! worktree is isolated at, `bdfba6d4c`). So what is fixed here is the part
//! `line-break: anywhere` owns on its own: an UNCONDITIONAL break
//! opportunity between every character, the same as `word-break: break-all`,
//! reachable WITHOUT `word-break` in the declaration at all.

use crate::table::tests::{geometria, rect, textos};

/// Four Ahem characters ("abcd") in a 2-character box: `line-break: anywhere`
/// alone (no `word-break`) must split into two lines of two characters each,
/// exactly like `word-break: break-all` would. Ahem is exact 1em/glyph, so
/// `width: 40px` at `font: 20px Ahem` is precisely two characters — the
/// binary search in `prefixo_que_cabe` has no rounding to hide behind.
#[test]
fn line_break_anywhere_alone_splits_every_two_ahem_chars() {
    let (_dom, list) = geometria(
        "<div style='font:20px/1 Ahem;width:40px;line-break:anywhere'>abcd</div>",
        800.0,
    );
    let pieces = textos(&list);
    assert_eq!(
        pieces,
        vec!["ab", "cd"],
        "line-break:anywhere alone must break every 2 characters: {pieces:?}"
    );
}

/// The same question as `break_all_parte_uma_palavra_que_break_word_deixaria_descer`
/// (`inline_box/tests/quebra.rs`), but for `line-break: anywhere`: it breaks a
/// SHORT word that would fit whole on the next line — which
/// `overflow-wrap: break-word` (and the absent `word-break` here) would let
/// drop down. Pins that `quebra_dentro` reads `line-break` as a third path
/// to `BreakWithin::Always`, not only `word-break: break-all`.
#[test]
fn line_break_anywhere_breaks_a_word_that_break_word_would_drop_down() {
    let narrow = "width:60px;font-size:16px";
    let (d1, l1) = geometria(
        &format!("<div style='{narrow};overflow-wrap:break-word'>aaaa <span>bbbb</span></div>"),
        800.0,
    );
    let (d2, l2) = geometria(
        &format!("<div style='{narrow};line-break:anywhere'>aaaa <span>bbbb</span></div>"),
        800.0,
    );
    let with_break_word = rect(&d1, &l1, "span", 0);
    let with_anywhere = rect(&d2, &l2, "span", 0);
    assert!(
        with_anywhere.h > with_break_word.h,
        "line-break:anywhere splits 'bbbb' into more lines than break-word, \
         which lets it drop down whole: anywhere={with_anywhere:?} break_word={with_break_word:?}"
    );
}
