//! Patterns that parse as expressions and are not programs.
//!
//! A regular expression literal is the one literal that carries a program in
//! its text. The lexer's job between the slashes is to find the closing one, so
//! everything about the pattern's own grammar is decided afterwards — and every
//! rule it breaks is an early error rather than a match that fails.
//!
//! Each rule is tested from both sides, and the valid side matters more here
//! than anywhere else in the checker: refusing a pattern that works breaks a
//! program that runs everywhere, and half of what a unicode pattern forbids is
//! legal without the flag.

use rts_codegen::names::Names;
use rts_codegen::parse::{ParseError, parse_script};

#[track_caller]
fn refused(source: &str) -> String {
    let mut names = Names::new();
    match parse_script(source, &mut names) {
        Err(ParseError::Syntax(message)) => message,
        other => panic!("{source:?} was not refused: {other:?}"),
    }
}

#[track_caller]
fn accepted(source: &str) {
    let mut names = Names::new();
    if let Err(error) = parse_script(source, &mut names) {
        panic!("{source:?} is a valid program and was refused: {error}");
    }
}

#[test]
fn two_groups_of_one_name_must_be_in_different_alternatives() {
    assert!(refused("/(?<a>x)(?<a>y)/;").contains("two groups"));
    assert!(refused("/(?<a>x)(?:(?<a>y))/;").contains("two groups"));

    // ES2025: only one alternative can match, so only one group exists.
    accepted("/(?<a>x)|(?<a>y)/;");
    accepted("/(?:(?<a>x)|(?<a>y))z/;");
}

#[test]
fn a_backreference_names_a_group_that_exists() {
    assert!(refused(r"/(?<a>x)\k<b>/;").contains("names no group"));
    assert!(refused(r"/\k<a>(?<ab>x)/;").contains("names no group"));

    // A backreference may precede its group.
    accepted(r"/\k<a>(?<a>x)/;");
    // Without `u`, `\k` is the letter `k` when the pattern names no group at
    // all — Annex B, and a rule that did not know it would refuse this.
    accepted(r"/\k/;");
}

#[test]
fn a_quantifier_needs_something_to_repeat() {
    assert!(refused("/?/;").contains("nothing to repeat"));
    assert!(refused("/{2}/;").contains("nothing to repeat"));
    assert!(refused("/{2,3}/;").contains("nothing to repeat"));
    // A lookbehind matches no characters, so repeating it means nothing.
    assert!(refused("/.(?<=.)?/;").contains("cannot be quantified"));
    // A lookahead may be repeated without `u`, which is Annex B and is what
    // the web does.
    accepted("/.(?=.)?/;");
    assert!(refused("/.(?=.)?/u;").contains("cannot be quantified"));

    assert!(refused("/x{3,2}/;").contains("counts down"));
    accepted("/x{2,3}/; /x{2,}/; /x{2}/;");
    // Without `u`, a `{` that opens no quantifier is an ordinary character.
    accepted("/x{/;");
    assert!(refused("/{/u;").contains("lone `{`"));
}

#[test]
fn a_unicode_pattern_forbids_what_annex_b_allows() {
    assert!(refused(r"/\c0/u;").contains("is not followed by a letter"));
    assert!(refused(r"/\M/u;").contains("no meaning"));
    assert!(refused(r"/\1/u;").contains("names no group"));
    assert!(refused(r"/\u{110000}/u;").contains("past the last code point"));
    assert!(refused(r"/[\d-a]/u;").contains("single characters"));
    assert!(refused(r"/[%-\d]/u;").contains("single characters"));

    // All of them are ordinary patterns without the flag.
    accepted(r"/\c0/; /\M/; /\1/; /[\d-a]/; /[%-\d]/;");
    // And a backreference to a group that exists is fine with it.
    accepted(r"/(x)\1/u;");
}

#[test]
fn the_flags_are_a_set_of_known_letters() {
    // SWC reaches the repeated and the unknown letter first, with a message of
    // its own; what is pinned is that the program is refused. The pair `u` and
    // `v` it accepts, because neither letter is wrong on its own.
    refused("/x/gg;");
    refused("/x/q;");
    assert!(refused("/x/uv;").contains("cannot both be given"));
    accepted("/x/dgimsy;");
    accepted("/x/gu;");
}

#[test]
fn a_modifier_group_has_to_mean_something() {
    assert!(refused("/(?i-i:a)/;").contains("both added and removed"));
    assert!(refused("/(?-:a)/;").contains("adds and removes nothing"));
    assert!(refused("/(?ii:a)/;").contains("given twice"));
    assert!(refused("/(?ms-i)/;").contains("not a modifier"));
    assert!(refused("/(?-Q:a)/;").contains("not a modifier"));

    // One empty half is legal: `(?s-:x)` adds `s` and removes nothing. Only
    // both halves empty changes nothing and is refused.
    accepted("/(?i:a)/; /(?i-ms:a)/; /(?-s:a)/; /(?s-:a)/;");
}

#[test]
fn ordinary_patterns_are_left_alone() {
    // The failure mode that matters. Every one of these is something a real
    // program writes, and a checker that refused any of them would be worse
    // than no checker at all.
    accepted(r"/^[a-z0-9_.-]+@[a-z0-9-]+\.[a-z]{2,}$/i;");
    accepted(r"/(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/;");
    accepted(r"/\s*([^,]+?)\s*(?:,|$)/g;");
    accepted(r"/(?<year>\d{4})-(?<month>\d{2})/u;");
    accepted(r"/[A-Z\u{1F600}]/u;");
    accepted(r"/\p{Script=Greek}+/u;");
    accepted(r"/a(?!b)(?<!c)d/;");
    accepted(r"/[\]\[\^-]/;");
    accepted(r"/x{1,}?y+?z*?/;");
}

#[test]
fn the_v_flag_is_a_different_class_grammar() {
    // `v` is not `u` and more. Its classes nest, subtract and intersect, and
    // hold string literals — none of which `u`'s grammar has. This module does
    // not read that grammar, so it asks none of `u`'s class questions under it
    // rather than answering them wrongly.
    accepted(r"/^[\d--\d]+$/v;");
    accepted(r"/^[\d--[0-9]]+$/v;");
    accepted(r"/^[[0-9]--\q{0|2|4}]+$/v;");
    accepted(r"/^[\d&&\p{ASCII_Hex_Digit}]+$/v;");

    // The same shapes under `u` are refused, which is what makes the two modes
    // worth telling apart.
    assert!(refused(r"/[\d--\d]/u;").contains("single characters"));

    // The rules that are about neither grammar still apply under `v`.
    assert!(refused(r"/(?<a>x)(?<a>y)/v;").contains("two groups"));
}
