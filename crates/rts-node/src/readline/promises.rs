//! `node:readline/promises` — `Interface.question()` as a `Promise`, and this
//! module's own `createInterface`.
//!
//! # The premise this crate's own audit asked to be checked first
//!
//! The parent module's previous top-of-file doc refused this whole module:
//! *"`Interface.question()`'s promise form … all need a Promise that resolves
//! LATER, from a native callback — this crate's entry surface can build an
//! already-settled promise but has no executor-style constructor."* That was
//! stale, not wrong-headed: `rts_core::entry::promise_new`/`promise_settle`
//! already exist and are already used from a native this exact way —
//! `stream/promises.rs`'s `pipeline`/`finished` mint a promise, hand a
//! settling closure to a callback-taking operation, and answer the promise
//! before it settles. [`question`] is that shape again: mint the promise,
//! hand `rl.once('line', settler)` a closure that settles it, answer the
//! promise. Nothing here is a second implementation of "wait for a line" —
//! [`super::on_data`] still does the only splitting there is.
//!
//! # `Interface`, and why THIS module builds a second prototype
//!
//! Real Node's `readline/promises` interface is not `instanceof
//! require('node:readline').Interface` — it is its own class that happens to
//! share every method but one. This module keeps that: [`create_interface`]
//! builds (once, `make_prototype` is idempotent by name) a prototype named
//! `"InterfacePromises"`, carrying only `question`, chained via
//! `set_prototype_in` onto the PARENT module's `"Interface"` prototype — so
//! `close`, `pause`, `resume`, `setPrompt`, `getPrompt`, `prompt`, `write` are
//! all found by the ordinary chain walk, and only `question` resolves to the
//! one below (own-property lookup wins over an inherited one). Building a
//! second, unrelated `close`/`pause`/… would be a second answer to what
//! closing an interface does, which is exactly what the parent module's own
//! doc already refuses.
//!
//! # Not implemented, by name
//!
//! - **`readlinePromises.Readline`** — the queued cursor-actions class whose
//!   `rl.cursorTo(...).clearLine(...).commit()` batches writes and answers a
//!   `Promise<void>`. `commit()` needs the same later-resolving promise this
//!   file's `question` now has, so the remaining gap is the class itself (the
//!   queue and the batched write), not the promise machinery.
//! - **`options.signal`** on `question()` — the parent module's own
//!   "Not implemented" list already states this for the callback form; the
//!   promise form inherits the same gap rather than closing it.
//! - Everything else the parent module's doc already names —
//!   `emitKeypressEvents`, history, `completer`, `crlfDelay` timing,
//!   `rl.write(data, key)` — applies here unchanged, since [`create_interface`]
//!   wires `input` through the exact same [`super::build_interface`].

use rts_core::entry::{self, Context, Provided};

/// `Interface.question()`, promise form — the only member of `InterfacePromises`.
const MEMBERS: &[(&str, Provided)] = &[("question", question)];

/// The namespace `node:readline/promises` is.
pub(super) fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("createInterface", create_interface)];
    entry::make_namespace(context, members)
}

/// `readlinePromises.createInterface(options)` — see the module doc for why
/// this mints its own `"InterfacePromises"` prototype rather than reusing the
/// callback form's.
pub(super) extern "C" fn create_interface(_e: u64, _this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let prototype = entry::with_runtime(|context| {
        // Empty `members`, the "chain-read" idiom `make_prototype`'s own doc
        // names: this READS the "Interface" prototype `readline::namespace`
        // already owns, rather than claiming it a second time. Passing
        // `super::INTERFACE_METHODS` here — a real, non-empty table — used to
        // make this call race `mod.rs`'s own registration for ownership of
        // "Interface", and lost every time: `mod.rs::namespace` always runs
        // first (`lib.rs` reads `node:readline`'s `.promises` member to wire
        // this very specifier, which forces it), so this call was never the
        // first and always the "different file" the guard panics on — every
        // `readlinePromises.createInterface()` call aborted the process.
        // `lib.rs`'s comment is why the read is safe rather than lucky: the
        // prototype is guaranteed to exist by the time any program can reach
        // this function at all.
        let base = entry::make_prototype(context, "Interface", &[]);
        let promised = entry::make_prototype(context, "InterfacePromises", MEMBERS);
        entry::set_prototype_in(context, promised, base);
        promised
    });
    super::build_interface(options, prototype)
}

/// `rl.question(query, options?)` — a promise for the next line, exactly
/// `rl.once('line', settler)` where `settler` settles the promise instead of
/// calling back. Writes `query` to `output` first, the same as the callback
/// form.
pub(super) extern "C" fn question(_e: u64, this: u64, query: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    let (output, once_method, line_name) = entry::with_runtime(|context| {
        (
            entry::get_member(context, this, "output"),
            entry::get_member(context, this, "once"),
            entry::make_string(context, "line"),
        )
    });
    super::write_to(output, query);
    // Minted before anything that could observe it, and outside every borrow
    // above — `promise_new`/`closure_new` each take the runtime borrow
    // themselves, and taking it twice aborts the process rather than failing.
    let promise = entry::promise_new();
    let settler = entry::closure_new(settle as *const () as usize as i64, promise);
    let absent = entry::undefined_value();
    entry::call(once_method, this, line_name, settler, absent, absent);
    promise
}

/// Settles the promise [`question`] minted, with the line `'line'` carried.
extern "C" fn settle(promise: u64, _this: u64, line: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::promise_settle(promise, line, 0);
    entry::undefined_value()
}
