//! `print`, `println`, `write` and `prompt` — the process's own input and
//! output, reachable with no import line.
//!
//! # Why these are globals and not a module
//!
//! They were `rts.io.print` and so on, reached through a nested namespace off
//! the `rts` module. Measured against this repository's own suite, that shape
//! was not being used: 224 of the 818 files imported `rts`, and exactly ONE
//! called `io.print`. What the other files do instead is define a local `print`
//! of their own — which is the shape a program actually writes, and which a
//! global serves directly.
//!
//! A local declaration still shadows one of these, as it does any global, so
//! every one of those files keeps working unchanged.
//!
//! # `prompt` is here and `console` is not
//!
//! `console` is a Node/browser surface with a format string, levels and a
//! stream per level; `rts-std-rwk`'s `console.rs` is that. These four are the
//! plain thing underneath — bytes to a stream, a line from stdin — with no
//! formatting rules to get wrong. Keeping them apart is what stops `print` from
//! growing a `%s` it does not have.
//!
//! # Not implemented, by name
//!
//! `prompt`'s default value and its browser-style cancel (there is no dialogue
//! to cancel; EOF answers `null`). Formatting of any kind: `print(a, b)` writes
//! the FIRST value and ignores the rest, because a separator is a decision
//! `console.log` already made differently and two answers to it would be worse
//! than one gap. An object prints as `[object]`, for the reason `console.rs`
//! states: its `toString` is user code a native cannot call from here.

use std::io::{BufRead, Write};

use rts_core_rwk::entry::{Context, Provided};

/// Installs them.
pub fn install(context: &mut Context) {
    let members: &[(&str, Provided)] = &[
        ("print", print),
        ("println", println_),
        ("write", print),
        ("prompt", prompt),
    ];
    for (name, code) in members {
        let value = rts_core_rwk::entry::make_callable(context, *code);
        rts_core_rwk::entry::declare_global(context, name, value);
    }
}

/// The text of a value, the way this crate prints one everywhere.
fn described(value: u64) -> String {
    rts_core_rwk::entry::described(value).unwrap_or_else(|| "[object]".to_owned())
}

/// `print(x)` — the text of a value, with no newline.
extern "C" fn print(_e: u64, _this: u64, value: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    // Flushed, because there is no newline to make a line-buffered stream do it:
    // a `print` whose output appears only when something else prints is a
    // program that looks like it hung.
    print!("{}", described(value));
    let _ = std::io::stdout().flush();
    rts_core_rwk::entry::undefined_value()
}

/// `println(x)` — the same, with one.
extern "C" fn println_(_e: u64, _this: u64, value: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    println!("{}", described(value));
    rts_core_rwk::entry::undefined_value()
}

/// `prompt(message?)` — one line from standard input, or `null` at end of file.
///
/// The message is written WITHOUT a newline and flushed, so the answer is typed
/// on the same line. `null` rather than `""` for end of file: an empty line is a
/// real answer a program may act on, and reading both as the same thing is a
/// wrong answer that runs.
extern "C" fn prompt(_e: u64, _this: u64, message: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let absent = rts_core_rwk::entry::undefined_value();
    if message != absent {
        print!("{}", described(message));
        let _ = std::io::stdout().flush();
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
    if read == 0 {
        return rts_core_rwk::entry::null_value();
    }
    // The terminator is not part of the answer, and `trim_end` would also eat a
    // trailing space the user typed on purpose.
    let text = line
        .strip_suffix('\n')
        .map_or(line.as_str(), |rest| rest.strip_suffix('\r').unwrap_or(rest));
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}
