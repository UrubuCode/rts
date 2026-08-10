//! `console` — the global every runtime has and no specification defines.
//!
//! # Why it is a global and not a module
//!
//! Because a program writes `console.log(x)` with no import line. That makes it
//! the host's to put on the global object rather than something a specifier
//! resolves to, which is why it is here and not beside `io`.
//!
//! # What it prints for an object
//!
//! [`inspect`] — Node-shaped: `{ a: 1, b: { c: [ 1, 2, 3 ] } }`, arrays as
//! `[ 1, 2, 3 ]`, a `Map`/`Set` as `Map(1) { 'k' => 'v' }`, depth capped at 2 and
//! a cycle printed as `[Circular *1]` rather than walked forever. The doc
//! comment this replaces said an object's `toString` is user code the runtime
//! cannot call from an entry point — true when it was written, and no longer:
//! the catchable-throw discipline (`rts-core` README rule 8) means a native
//! CAN call into the program now and ask whether it threw afterward, which
//! `inspect::entries_via_for_each` does to read a `Map`/`Set`. What stays true is
//! narrower than the old comment: `inspect` never calls a `toString` — Node's
//! own `util.inspect` does not either, an object prints from its own properties
//! — it calls `forEach` for a collection and a getter only when a property read
//! reaches one.
//!
//! # Arguments past the fourth
//!
//! Read through [`entry::arguments_at`], the same entry point that fixed
//! `String.fromCharCode`, `Math.max` and `Array.prototype.push` over the same
//! four-slot convention: past the fourth, the compiler already spilled the rest
//! into a vector this reads back rather than losing.
//!
//! # Not implemented, by name
//!
//! `table` — a table that does not draw a table is worse than no `table`, and
//! this crate's owner has rejected stub surfaces before. `%c` (CSS styling: no
//! terminal here to style). Depth/color options `util.inspect` itself takes —
//! `console.log` calls it with none.

mod format;
mod inspect;
mod state;

use rts_core::entry::{self, Context, Provided};

/// Installs `console` as a global.
pub fn install(context: &mut Context) {
    let members: &[(&str, Provided)] = &[
        ("log", log),
        ("info", log),
        ("debug", log),
        ("warn", warn),
        ("error", warn),
        ("trace", trace),
        ("dir", dir),
        ("group", group),
        ("groupCollapsed", group),
        ("groupEnd", group_end),
        ("time", time),
        ("timeEnd", time_end),
        ("timeLog", time_end),
        ("count", count),
        ("countReset", count_reset),
        ("assert", assert),
    ];
    let console = rts_core::entry::make_namespace(context, members);
    rts_core::entry::declare_global(context, "console", console);
}

/// `console.log(a, b, c, …)` — space separated (with `%`-formatting, when the
/// first argument is a string that has any), to standard output.
extern "C" fn log(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    print(&given(a, b, c, d));
    entry::undefined_value()
}

/// `console.error(…)` / `console.warn(…)` — the same, to standard error.
///
/// Which stream is not decoration: a harness that captures output separates the
/// two, and sending both to stdout would make a program's diagnostics
/// indistinguishable from its answers.
extern "C" fn warn(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    eprint(&given(a, b, c, d));
    entry::undefined_value()
}

/// `console.trace(…)` — the message, prefixed `Trace:`, and the call stack
/// `Error().stack` already knows how to build.
extern "C" fn trace(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let values = given(a, b, c, d);
    let (Ok(message), Ok(frames)) = (format::line(&values), inspect::stack_frames()) else {
        return entry::undefined_value();
    };
    eprintln!("{}Trace: {message}{frames}", state::indent());
    entry::undefined_value()
}

/// `console.dir(value)` — one value, always through `inspect` rather than
/// through `%`-formatting (a string is never a format string to `dir`).
extern "C" fn dir(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match inspect::top_level(value) {
        Ok(text) => println!("{}{text}", state::indent()),
        Err(inspect::Poisoned) => {}
    }
    entry::undefined_value()
}

/// `console.group(label?)` — the label, then every following line indented
/// two spaces further until `groupEnd`.
extern "C" fn group(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let values = given(a, b, c, d);
    if !values.is_empty() {
        print(&values);
    }
    state::push_group();
    entry::undefined_value()
}

extern "C" fn group_end(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    state::pop_group();
    entry::undefined_value()
}

/// `console.time(label?)` — starts a stopwatch under a label, `"default"` when
/// none is given, matching Node.
extern "C" fn time(_e: u64, _this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    state::start_timer(&label_of(label));
    entry::undefined_value()
}

/// `console.timeEnd(label?)` / `console.timeLog(label?)` — the elapsed time
/// since `time`, or a note that the label was never started.
extern "C" fn time_end(_e: u64, _this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = label_of(label);
    match state::read_timer(&name) {
        Some(elapsed) => println!("{}{name}: {:.3}ms", state::indent(), elapsed.as_secs_f64() * 1000.0),
        None => eprintln!("{}Timer '{name}' does not exist", state::indent()),
    }
    entry::undefined_value()
}

/// `console.count(label?)` — how many times this label has been counted, this
/// process.
extern "C" fn count(_e: u64, _this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = label_of(label);
    let seen = state::bump_count(&name);
    println!("{}{name}: {seen}", state::indent());
    entry::undefined_value()
}

extern "C" fn count_reset(_e: u64, _this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    state::reset_count(&label_of(label));
    entry::undefined_value()
}

/// `console.assert(condition, …message)` — prints only when `condition` is
/// falsy, prefixed `Assertion failed`, to standard error. Never throws: Node's
/// does not either.
extern "C" fn assert(_e: u64, _this: u64, condition: u64, b: u64, c: u64, d: u64) -> u64 {
    if entry::to_boolean(condition) {
        return entry::undefined_value();
    }
    let rest = entry::with_runtime(|context| entry::arguments_at(context, 0, [b, c, d, entry::undefined_in(context)]));
    match format::line(&rest) {
        Ok(text) if text.is_empty() => eprintln!("{}Assertion failed", state::indent()),
        Ok(text) => eprintln!("{}Assertion failed: {text}", state::indent()),
        Err(inspect::Poisoned) => {}
    }
    entry::undefined_value()
}

/// The arguments a call actually carried, past the convention's four slots.
fn given(a: u64, b: u64, c: u64, d: u64) -> Vec<u64> {
    entry::with_runtime(|context| entry::arguments_at(context, 0, [a, b, c, d]))
}

/// A label argument's text, `"default"` for `undefined` — Node's own default,
/// so `console.time()` and `console.timeEnd()` with no argument pair up.
fn label_of(value: u64) -> String {
    entry::text_of(value).unwrap_or_else(|| "default".to_owned())
}

fn print(values: &[u64]) {
    match format::line(values) {
        Ok(text) => println!("{}{text}", state::indent()),
        Err(inspect::Poisoned) => {
            // A throw is in flight from reading one of the arguments (a getter,
            // or a `Map`/`Set`'s own `forEach`). Printing nothing and returning
            // is rule 8: the compiled call site above this one asks `thrown()`
            // next and re-raises it, which a partial line here would not change
            // and would only make harder to read alongside.
        }
    }
}

fn eprint(values: &[u64]) {
    match format::line(values) {
        Ok(text) => eprintln!("{}{text}", state::indent()),
        Err(inspect::Poisoned) => {}
    }
}
