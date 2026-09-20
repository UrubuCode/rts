//! JSONC is what `tsconfig.json` and `package.json` are written in.

use rts_host::jsonc::strip;

#[test]
fn a_line_comment_goes_and_the_newline_stays() {
    assert_eq!(strip("{\n  // a\n  \"x\": 1\n}"), "{\n  \n  \"x\": 1\n}");
}

#[test]
fn a_block_comment_goes() {
    assert_eq!(strip("{/* a */\"x\": 1}"), "{\"x\": 1}");
}

/// The case the `//`-only version got wrong, and the reason this moved: a
/// `tsconfig.json` written by `tsc --init` is full of block comments.
///
/// The three newlines are the point: one before the comment, one the
/// comment spanned, one after it closed. A block comment that swallowed
/// the line it ended on would shift every line number below it.
#[test]
fn a_block_comment_spanning_lines_goes() {
    assert_eq!(strip("{\n/* a\n b */\n\"x\": 1}"), "{\n\n\n\"x\": 1}");
}

/// A trailing comma is legal in JSONC and fatal to `serde_json`.
#[test]
fn a_trailing_comma_goes_from_an_object_and_an_array() {
    assert_eq!(strip("{\"x\": [1, 2,],}"), "{\"x\": [1, 2]}");
}

/// What a stripper must never do: a comment marker inside a string is text.
#[test]
fn what_is_inside_a_string_is_left_alone() {
    assert_eq!(strip("{\"x\": \"a // b /* c */\"}"), "{\"x\": \"a // b /* c */\"}");
    assert_eq!(strip("{\"x\": \"a,\"}"), "{\"x\": \"a,\"}");
}

/// An escaped quote does not end the string, so what follows is still text.
#[test]
fn an_escaped_quote_does_not_end_the_string() {
    assert_eq!(strip("{\"x\": \"a\\\" // b\"}"), "{\"x\": \"a\\\" // b\"}");
}
