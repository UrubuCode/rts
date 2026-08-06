//! `console` — the global every runtime has and no specification defines.
//!
//! # Why it is a global and not a module
//!
//! Because a program writes `console.log(x)` with no import line. That makes it
//! the host's to put on the global object rather than something a specifier
//! resolves to, which is why it is here and not beside `io`.
//!
//! # What it prints for an object, and why that is stated
//!
//! `described` answers `ToString` of a primitive and `None` for an object,
//! because an object's `toString` is user code the runtime cannot call from an
//! entry point. So an object prints as `[object]` — visibly wrong rather than
//! quietly empty, and the thing `node:util`'s `inspect` exists to replace once
//! this can reach it.
//!
//! # Not implemented, by name
//!
//! `table`, `group`/`groupEnd`, `time`/`timeEnd`, `count`, `trace`, `dir`,
//! `assert`. Each answers `undefined`, which a program can see.

use rts_core_rwk::entry::{Context, Provided};

/// Installs `console` as a global.
pub fn install(context: &mut Context) {
    let members: &[(&str, Provided)] = &[
        ("log", log),
        ("info", log),
        ("debug", log),
        ("warn", warn),
        ("error", warn),
    ];
    let console = rts_core_rwk::entry::make_namespace(context, members);
    rts_core_rwk::entry::declare_global(context, "console", console);
}

/// `console.log(a, b, c, d)` — space separated, to standard output.
///
/// Four arguments, because the convention carries four and the receiver takes
/// one of the five slots. A call with more is refused at the site rather than
/// losing its arguments here.
extern "C" fn log(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    println!("{}", line(&[a, b, c, d]));
    rts_core_rwk::entry::undefined_value()
}

/// `console.error(…)` and `console.warn(…)` — the same, to standard error.
///
/// Which stream is not decoration: a harness that captures output separates the
/// two, and sending both to stdout would make a program's diagnostics
/// indistinguishable from its answers.
extern "C" fn warn(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    eprintln!("{}", line(&[a, b, c, d]));
    rts_core_rwk::entry::undefined_value()
}

/// The text of a call's arguments, space separated.
///
/// A trailing argument the caller did not write arrives as `undefined` from the
/// convention's padding and is dropped — otherwise every `console.log(x)` would
/// print three `undefined`s after it. A `undefined` a program passed
/// deliberately is dropped too, which is the cost of a fixed arity and is named
/// rather than hidden.
fn line(values: &[u64]) -> String {
    let absent = rts_core_rwk::entry::undefined_value();
    values
        .iter()
        .take_while(|value| **value != absent)
        .map(|value| {
            rts_core_rwk::entry::described(*value).unwrap_or_else(|| "[object]".to_owned())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
