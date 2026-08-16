//! The words the language keeps for itself, and the escapes strict code drops.
//!
//! Split out of the walk because none of it is a walk: each is a question about
//! one word or one piece of raw text, asked from [`super`] where the context
//! that decides it is known.

use crate::syntax::Directive;


/// Whether a word is a keyword in every context.
///
/// `null`, `true` and `false` are in the list because they are literals rather
/// than names, and the language reserves their spelling for the same reason it
/// reserves `if`: nothing else may be called that.
///
/// Absent, deliberately: `let`, `static`, `async`, `of`, `get`, `set` and the
/// rest of the contextual words. Each is a perfectly good identifier —
/// `var let = 1` is a program in sloppy code — and putting them here would
/// refuse programs that run everywhere.
pub(super) fn is_reserved(word: &str) -> bool {
    matches!(
        word,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

/// Whether a word is reserved only where the code is strict.
///
/// These are the future reserved words. They name things perfectly well in
/// sloppy code — jQuery shipped a `private` for years — and strict mode is what
/// takes the spelling away, so the answer depends on the context the walk
/// carries rather than on the word alone.
pub(super) fn is_reserved_in_strict(word: &str) -> bool {
    matches!(
        word,
        "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

/// A directive prologue that turns on strict mode and contains a legacy escape.
///
/// `function f() { "\1"; "use strict"; }` is not a program, and the reason is
/// the order it is *not* read in: a prologue is decided whole before any of it
/// means anything, so the `"use strict"` later in it makes the earlier string
/// strict code too. A reader going line by line would see a legal string
/// followed by a directive.
///
/// This is the one rule that asks a literal for its raw text, and it is why
/// [`crate::syntax::Directive`] keeps it: `"1"` is the same *value* as
/// `"1"` and a different program, so the cooked string cannot answer.
pub(super) fn legacy_escape_in_a_strict_prologue(directives: &[Directive]) -> Option<String> {
    if !directives.iter().any(|directive| directive.is_use_strict()) {
        return None;
    }
    directives
        .iter()
        .find(|directive| has_legacy_escape(&directive.raw))
        .map(|_| "a legacy octal escape cannot appear in strict code".to_owned())
}

/// Whether a string's raw text contains an escape strict mode forbids.
///
/// Three shapes, and they are one rule: `\1` … `\7` are octal, `\8` and `\9`
/// are the "non-octal decimal" escapes that exist only because the web has
/// them, and `\0` is fine alone and forbidden the moment a digit follows —
/// `\08` is the octal escape wearing a zero.
fn has_legacy_escape(raw: &str) -> bool {
    let text: Vec<char> = raw.chars().collect();
    let mut index = 0;
    while index < text.len() {
        if text[index] != '\u{5c}' {
            index += 1;
            continue;
        }
        match text.get(index + 1) {
            Some('0') => {
                if text.get(index + 2).is_some_and(char::is_ascii_digit) {
                    return true;
                }
            }
            Some('1'..='9') => return true,
            _ => {}
        }
        // Past the backslash and whatever it escaped, so that `\1` is a
        // backslash followed by a digit rather than an octal escape.
        index += 2;
    }
    false
}
