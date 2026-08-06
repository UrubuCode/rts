//! `rts` — the standard namespace, and the `io` inside it.
//!
//! # Why `io` is an object and not four functions
//!
//! Because that is how the corpus imports it: `import { io } from "rts"` and
//! then `io.print(x)`. The namespace holds `io`; `io` holds the functions. A
//! flat namespace would answer the import and fail at the first call, which is
//! the shape of gap that looks like it works.
//!
//! # What is here, and why so little
//!
//! What the suite reaches for, counted rather than imagined: `io` in 127 files
//! is the largest single member, and the rest — `promise`, `time`, `thread`,
//! `atomic`, `process`, `fs` — are in fewer than twenty each and every one of
//! them needs a mechanism this engine does not have yet. They are absent by
//! name rather than stubbed: a `time.now` answering zero would make a test that
//! measures duration pass for the wrong reason.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `rts` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let io = rts_core_rwk::entry::make_namespace(
        context,
        &[("print", print), ("println", println_), ("write", print)],
    );
    rts_core_rwk::entry::put_member(context, namespace, "io", io);
    namespace
}

/// `io.print(x)` — the text of a value, with no newline.
///
/// Through `described`, which is `ToString` of a primitive and `None` for an
/// object: an object prints as `[object]` rather than running a `toString` the
/// runtime cannot call from here. Named, because a harness that printed the
/// wrong thing quietly would make a test comparing output pass for the wrong
/// reason.
extern "C" fn print(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    print!("{}", text_of(value));
    rts_core_rwk::entry::undefined_value()
}

/// `io.println(x)`.
extern "C" fn println_(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    println!("{}", text_of(value));
    rts_core_rwk::entry::undefined_value()
}

/// What a value prints as.
fn text_of(value: u64) -> String {
    rts_core_rwk::entry::described(value).unwrap_or_else(|| "[object]".to_owned())
}
