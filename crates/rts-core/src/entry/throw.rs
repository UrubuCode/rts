//! A thrown value that no handler in the throwing function caught.
//!
//! # What this used to do, and why it changed
//!
//! It ended the program. The reasoning was sound and its boundary was drawn
//! honestly: the machine computes where a throw lands from the region tree of
//! the function it is IN, which is complete for every handler in that function
//! and says nothing about a caller's — so a `try` protecting a call was refused
//! by name rather than compiled into a `catch` that silently never runs.
//!
//! What that reasoning assumed is that finding a caller's handler means
//! unwinding the native stack, which needs an exception table and a personality
//! routine. It does not. It needs the throw to leave ONE frame and the caller to
//! ask whether it did — and the caller is compiled code that can be made to ask.
//!
//! So a throw is now **recorded** rather than reported, the machine returns from
//! the throwing function instead of trapping, and every call site checks. A
//! throw that reaches a handler is caught by the machine's own region tree,
//! unchanged; a throw that reaches nobody arrives here still pending when the
//! program ends, and [`pending`] is what the host reads to report it exactly as
//! this file used to.
//!
//! # Why one slot and not a stack
//!
//! Because there is one throw in flight. A second cannot start before the first
//! is caught or ends the program — the only code that runs in between is
//! compiled code returning, and returning cannot throw. A stack would be
//! modelling a state the language cannot reach.
//!
//! A cleanup that throws while unwinding IS such a state, and it is named in
//! [`record`]: the second value replaces the first, which is what JavaScript
//! says a `finally` that throws does to the exception it was unwinding.
//!
//! # The alternative this refused now EXISTS in the code generator
//!
//! [`thrown`] calls it "an exception table and a personality routine, which is a
//! campaign", and when that was written the campaign would have started from
//! nothing. It would not now: Cranelift 0.131 — the version this workspace
//! builds against — carries `ir::exception_table` and the `try_call` /
//! `try_call_indirect` instructions, where a call names its normal successor
//! block and a set of catch destinations keyed by tag, and the compiled function
//! carries the metadata to find them.
//!
//! Read against what this file does, `try_call` is the same shape one level
//! down: `Inst::Call` followed by a load, a compare and a branch is a
//! hand-rolled `try_call` whose exception table is one entry wide and lives in a
//! thread-local instead of beside the code. The mechanism is not exotic and it
//! is no longer absent.
//!
//! **It is still not proposed, and the reason is not inertia.** Three things
//! would have to be true and none has been checked:
//!
//! 1. **A native has to be able to raise into it.** The throw is recorded by a
//!    Rust function in this crate, and `try_call` catches what the *unwinder*
//!    delivers. Rust panics do not cross an `extern "C"` boundary usefully and
//!    this crate raises without panicking on purpose, so something would have to
//!    carry a throw from a native body into an unwind — which is a second
//!    mechanism beside the slot, not a replacement for it.
//! 2. **x64 Windows in particular.** The engine's only shipping target unwinds
//!    through SEH, and whether Cranelift's exception support covers it here was
//!    not verified. Machine rule 12: unproven behaviour fails safely, and an
//!    unverified unwinder fails as a silently uncaught throw.
//! 3. **It has to be worth it where the check actually is.** What the check
//!    costs at a site is 0.1 to 0.6 ns (measured, `rts-codegen`'s
//!    `emit/expr.rs`). What it costs the *compiler* is larger and is the real
//!    argument — but `rts-codegen`'s `runtime::raising` collects a quarter of it
//!    by deleting checks that were never needed, which is a list of eight
//!    operations rather than an unwinder.
//!
//! So this is recorded as a **named, now-available** alternative with three open
//! questions, not as work. What would reopen it is a measurement showing the
//! remaining checks dominate something, and (2) answered.

use super::{Context, with_current};

/// Records a thrown value that no handler in the throwing function caught.
///
/// The tag is carried and not interpreted: the machine compares tags for
/// equality and this is the escaping path, where there was nothing to compare
/// against. It is kept so that a value thrown with an unexpected tag can still
/// be reported with the tag it had.
///
/// # Why the second throw wins
///
/// A `finally` that throws while a value is already unwinding replaces it, which
/// is what the language says. So this overwrites rather than refusing, and the
/// alternative — keeping the first — would make a `finally`'s own failure
/// invisible.
#[rtse::entry("rts_throw")]
pub fn throw(tag: i64, payload: u64) {
    super::current::set_thrown(Some((tag, payload)));
}

/// Whether a throw is in flight, as `1` or `0`.
///
/// # Why an integer and not a boolean
///
/// A Rust `bool` is one byte, and compiled code reading it as a word takes the
/// callee's leftover bits — the mistake that once made `===` answer true for two
/// different strings in release and false in debug. Every flag crossing this
/// boundary is an `i64` for that reason.
///
/// # Why this is a call at every call site
///
/// Because a throw has to leave the frame it was raised in, and the frame above
/// only learns that by asking. The alternative is unwinding the native stack
/// with an exception table and a personality routine, which is a campaign; this
/// is a load and a branch.
///
/// # Where the slot lives, and how much moving it was actually worth
///
/// It read the slot through [`with_current`] — a thread-local lookup, a
/// `RefCell` borrow, and an index into the context stack — to answer a
/// question that is `false` on essentially every call in a program that is not
/// unwinding. Since compiled code asks after *every* call that can raise, that
/// bookkeeping is paid twice per array element in a loop like `s = s + a[i]`:
/// once for the element read and once for this. So the slot moved to its own
/// thread-local cell in [`super::current`], and this became a thread-local
/// lookup and a load.
///
/// **It is a small win and not the systemic one it looked like.** Release,
/// `while (i < M) { s = s + nums[i]; i = i + 1; }`, 300 × 20 000, minimum of
/// seven timings per run, three runs per binary, the two binaries differing in
/// nothing but where the slot lives:
///
/// | | before | after |
/// |---|---|---|
/// | `s = s + nums[i]` | 23.00 / 23.17 / 23.17 ns/op | 21.83 / 21.83 / 21.83 |
/// | `s = s + 1.5` (no call at all) | 6.67 / 6.67 / 6.67 | 6.00 / 6.00 / 6.00 |
///
/// **Read the second row before believing the first.** That loop makes no
/// runtime call, so nothing here can have changed it — and it moved 0.67 ns
/// anyway, which is what code layout does to a measurement. The element loop
/// moved 1.3 ns. The honest attribution is therefore **0.6 to 1.3 ns per
/// element read, 3–6 %**, not the 1.3 ns the subtraction alone would claim.
///
/// What that leaves is where the cost actually is: an element read is ~16 ns
/// over a loop that calls nothing, and that is the entry point's own body, not
/// the asking afterwards. A cheaper flag was the wrong target; the element read
/// wants a machine capability.
///
/// It is not a cached flag beside the old field, because two answers to one
/// question is the drift this repository has a rule about.
#[rtse::entry("__rts_thrown")]
pub fn thrown() -> i64 {
    i64::from(super::current::thrown_pending())
}

/// Where this thread's flag word lives, so compiled code can LOAD it.
///
/// # What this is, against the paragraph above
///
/// [`thrown`] measured a cheaper flag as "a small win and not the systemic one
/// it looked like", and that measurement moved the flag out of the context and
/// into a thread-local — it did not remove the CALL. This does: the address is
/// asked once per activation and every check afterwards is one load, which is
/// what that entry's own documentation named as "the size of the prize for
/// making the flag a LOAD from a known address rather than a call".
///
/// Answered at run time rather than baked in while compiling, because both
/// constant forms are wrong at once: an object file runs in a different process
/// from the one that compiled it, and a data symbol of the module is shared
/// between threads while this word is per-thread. Asking on the thread that
/// will read it answers both, and the address is stable for that thread's life.
///
/// Still one source of one fact. `current::InFlight` is `repr(C)` and its first
/// word IS the discriminant the `Option` is derived from — not a copy of it
/// kept in step by hand, which is what [`thrown`] refused and this keeps
/// refusing.
#[rtse::entry("__rts_thrown_address")]
pub fn thrown_address() -> i64 {
    super::current::thrown_address() as i64
}

/// Takes the value in flight, clearing it.
///
/// Cleared by the take, because the caller is about to re-raise it: leaving it
/// set would make the re-raise look like a second throw to the next check, and
/// every call after a caught one would appear to be unwinding.
#[rtse::entry("__rts_take_thrown")]
pub fn take_thrown() -> u64 {
    match super::current::take_thrown_slot() {
        Some((_, payload)) => payload,
        // Reached only if compiled code asked without asking [`thrown`] first,
        // which the emitter does not do. `undefined` rather than a poison value:
        // there is no honest value here and the caller is about to throw it.
        None => with_current(|context| super::objects::undefined_of(context)),
    }
}

/// The throw still in flight when a program ended, if there was one.
///
/// Read by the host after the program returns. This is where an uncaught
/// exception is reported, which is what this module did inline until a throw
/// could leave a frame.
pub fn pending() -> Option<(i64, String)> {
    let (tag, payload) = super::current::take_thrown_slot()?;
    with_current(|context| {
        // Whatever text the value has WITHOUT running user code.
        //
        // `ToPrimitive` on an object calls a `toString` an entry point cannot
        // call, and reaching back into the program that just failed is not worth
        // a diagnostic. But an error object needs neither: `name` and `message`
        // are ordinary data properties, so `throw new Error("boom")` reports
        // `"Error: boom"` from a read rather than from a call — the difference
        // between a message that names the fault and one that says "an object".
        let value = crate::value::Value(payload);
        // `.stack` first, because it already carries the header AND the frames:
        // reporting `Error: boom` where the value knows it came through `inner`,
        // `middle` and `outer` throws away the half a reader needs. A data read,
        // like everything else here — the property was written when the error
        // was constructed.
        let traced = value.as_slot().and_then(|cell| {
            let key = context.well_known("stack");
            let found = super::objects::read_property(context, cell, key)?;
            super::text::to_text(context, found)?.to_rust()
        });
        if let Some(traced) = traced {
            return Some((tag, traced));
        }
        let described = super::text::to_text(context, value)
            .and_then(|text| text.to_rust())
            .or_else(|| super::error::joined(context, value.as_slot()?));
        Some((tag, described.unwrap_or_else(|| "an object".to_owned())))
    })
}

/// Whether a throw is in flight, without taking it.
///
/// # Who this is for, and why it is not [`thrown`]
///
/// [`thrown`] is an entry point: compiled code calls it after every operation
/// that can raise, and it answers an `i64` because that is what crosses the ABI.
/// This is the RUNTIME's own spelling, for a native that called back into user
/// code and has to decide what to do with the answer.
///
/// It does not clear, because a native that sees one is propagating rather than
/// handling: it returns early and the check above it sees the same throw. A
/// native that CLEARS one has decided it is the handler, and that is
/// [`caught`].
///
/// # The rule this exists to make writable
///
/// **A native that calls user code must ask whether the callee left a throw
/// behind before it looks at the answer.** Without it the answer is `undefined`
/// — what `invoke` returns when it did not run — and `undefined` is a value, so
/// the native carries on. That is how `[...b]` with a throwing `next` became an
/// endless loop filling a vector: `done` read `undefined`, which is never true.
pub(in crate::entry) fn in_flight() -> bool {
    super::current::thrown_pending()
}

/// The value in flight, taken, for a native that is handling it.
///
/// `None` when nothing was thrown, which is the ordinary path. Taking is what
/// separates this from [`in_flight`]: whoever calls this has decided the throw
/// stops here, and leaving it set would make the next check see a throw that
/// nobody is unwinding any more.

pub(in crate::entry) fn caught() -> Option<u64> {
    super::current::take_thrown_slot().map(|(_, payload)| payload)
}

/// The tag a JavaScript throw carries.
///
/// One value, and it is `rts-codegen`'s `protect::JS_THROW` seen from this side.
/// The machine compares tags for equality and does not interpret them, so what
/// matters is that a throw the RUNTIME raises carries the same number a throw
/// the PROGRAM raises does — otherwise a `catch` written in the program would
/// not match a `TypeError` this crate produced, and the mismatch would look like
/// a handler that mysteriously does not run.
///
/// It is stated here rather than shared because the two crates are not allowed
/// to depend on each other in that direction: this is the runtime and that is
/// the language. What keeps them in step is that both are one line, both name
/// the other, and `crates/rts-host` asserts the pair — the same shape as
/// every other agreement across this boundary.
const JS_THROW: i64 = 1;

/// Raises a `TypeError` a `catch` in the program can see.
///
/// # Why a native may do this at all now
///
/// It could not before, and the reason was not the raising: it was that no
/// native ASKED. `invoke` answers `undefined` for a call that threw, `undefined`
/// is a value, and every native that called user code carried on with it — a
/// spread over a throwing iterator became an endless loop, and a `.then` handler
/// that threw fulfilled its derived promise. Those checks exist now, which is
/// what makes this safe to reach for rather than a trap.
///
/// The message is built with the program's own `TypeError`, so `e instanceof
/// TypeError` holds and `e.message` reads what was passed. A second error shape
/// invented here would be an object that fails both.
/// Raises a `TypeError` a `catch` in the program can see, from a HOST crate.
///
/// `rts-node` needs this: `assert.ok(0)`, `Buffer.alloc(-1)` and
/// `execSync` of a failing command all have to throw, and every one of them
/// answered a value instead — the tests for them assert `threw === true` and
/// were reading `false`.
///
/// Public where `type_error` is not, and the difference is the argument: this
/// takes a MESSAGE and nothing else. A caller outside this crate cannot name the
/// throw tag or build the error object, which is what keeps the two agreements
/// this file owns — which number a `catch` matches, and what an error IS — from
/// leaking into a crate that would then have a second answer to either.
pub fn throw_type_error(message: &str) {
    type_error(message);
}

/// Raises a value a `catch` in the program can see, whatever the value is.
///
/// # Why this is public where the tag is not
///
/// [`throw_type_error`]'s doc says a caller outside this crate cannot name the
/// throw tag or build the error object, and that stays true: this takes the
/// VALUE and supplies the tag itself. The two agreements this file owns —
/// which number a `catch` matches, and what an error IS — do not move.
///
/// It exists because `throw new Error(…)` is not the only throw a language has.
/// `napi_throw` hands over whatever the addon built, which may be a string, a
/// number, or an object with no `Error` anywhere in it, and JavaScript is
/// emphatic that all three are throwable.
pub fn throw_value(value: u64) {
    super::current::set_thrown(Some((JS_THROW, value)));
}

/// Builds one of the language's own error objects, without throwing it.
///
/// `None` for a name this engine does not provide, which is the honest answer
/// rather than a plain object wearing the name: an addon asking for a
/// `SystemError` should learn that it did not get one, not receive something
/// that fails `instanceof`.
///
/// The construction is [`named_error`]'s, extracted rather than copied — the
/// two would otherwise disagree about whether the class is registered on demand,
/// which was already a bug once (the throw was silently dropped and it looked
/// like `try`/`catch` not working).
pub fn make_named_error(name: &str, message: &str) -> Option<u64> {
    let made = with_current(|context| {
        let constructor = match super::class_support::made(context, name) {
            Some(constructor) => constructor,
            None => super::error::provided(name)?(context),
        };
        let text = context
            .intern_value(crate::text::Str::from_str(message))
            .bits();
        Some((constructor, text))
    })?;
    let (constructor, text) = made;
    let absent = with_current(|context| super::objects::undefined_of(context));
    Some(super::functions::construct(
        constructor,
        text,
        absent,
        absent,
        absent,
    ))
}

pub(in crate::entry) fn type_error(message: &str) {
    named_error("TypeError", message);
}

/// Raises a `RangeError` a `catch` in the program can see.
///
/// Same construction as [`type_error`], for the argument-range checks that
/// live outside `functions.rs` — `"x".repeat(-1)` and the like.
pub(in crate::entry) fn range_error(message: &str) {
    named_error("RangeError", message);
}


/// Raises a plain `Error` a `catch` in the program can see.
///
/// For a failure that is about none of the three things the errors above name —
/// not a value's type, not a range, not malformed text. Module resolution is the
/// first: `import x from "m"` for an `m` nothing registered is not a wrong
/// value, it is a name with nothing behind it, and it has to fail where it is
/// written. See [`super::modules::module_binding`] for what it answered before.
pub(in crate::entry) fn plain_error(message: &str) {
    named_error("Error", message);
}

/// Raises a `SyntaxError` a `catch` in the program can see.
///
/// Same construction as [`type_error`], for malformed *input* rather than a
/// misused value. Two places reach it, and both are text a program computed:
/// `JSON.parse` of something that is not JSON, and a pattern `new RegExp(…)`
/// cannot build.
///
/// The second was answering `undefined` instead, which is worse than it looks:
/// `new` on a callable that answers a primitive gives back the half-built
/// `this`, so `new RegExp("[]")` produced an object with no `source`, no
/// `flags` and no compiled pattern. The program carried on and died several
/// lines later reading `.exec` off it — naming neither the pattern nor the line
/// that wrote it.
pub(in crate::entry) fn syntax_error(message: &str) {
    named_error("SyntaxError", message);
}

/// Raises a `ReferenceError` a `catch` in the program can see.
///
/// Same construction as [`type_error`], for the one read the language itself
/// says throws: a name nothing declares and nothing provides. `rts-codegen`
/// decides, while compiling, which names those are — everything a scope binds,
/// everything [`super::global`]'s `PROVIDED` list or a sloppy-mode write
/// installs — and reaches this only for a name it could prove is neither. That
/// is what keeps a typo compiled against a name that IS one of those two sets
/// looking exactly as it did before: nothing about this reaches a name a scope
/// could have meant, only the ones no scope offers at all.
pub(in crate::entry) fn reference_error(message: &str) {
    named_error("ReferenceError", message);
}

/// Builds and raises `new <name>(message)`, shared by every named-error site.
///
/// One function rather than one copy per class, because the three differ only
/// in which constructor they look up — repeating the borrow, the registration
/// fallback and the `thrown` write per class is the kind of duplication rule 3
/// of this crate's README warns turns into three answers that drift.
fn named_error(name: &str, message: &str) {
    // Through `make_named_error` rather than beside it. The two differ only in
    // whether the result is thrown, and a second copy of "how an error is
    // built" is how one of them comes to register the class on demand and the
    // other not — which was a real bug once: the class was never registered,
    // the throw was silently dropped, and it looked like `try`/`catch` not
    // working.
    if let Some(error) = make_named_error(name, message) {
        throw_value(error);
    }
}

/// What each compiled function is called, by its code address.
///
/// Seeded by the host once the program is placed, for the reason the frame
/// shapes are: an address is not a number anybody holds until then.
///
/// EVERY function is here, with its declared arity beside the name, because
/// `f.length` is a property the language promises for all of them and
/// `(function(){}).name` is the empty string rather than an absence. It held
/// only the NAMED ones, and a stack trace was the reason — so the trace is
/// where that filter lives now: an empty name prints no line, which is a rule
/// about traces rather than about functions.
pub fn declare_function_names(context: &mut Context, names: Vec<(u64, String, u32, bool)>) {
    context.function_names = names;
}

/// The call stack, in the shape Node and Bun print it.
///
/// ```text
///     at inner
///     at outer
/// ```
///
/// # Where the stack comes from
///
/// `functions::invoke` already pushes the callable it is about to jump to, so
/// that a bound function can know which binding it is. That list IS the stack —
/// innermost last — and this reads it rather than keeping a second one, which
/// would be two records of one fact that disagree the first time either forgets
/// to pop.
///
/// # What it does not say yet, and why it is still worth having
///
/// A LINE. The machine records a source position per instruction and the code
/// generator carries it, but nothing maps an address back to one at run time —
/// that is `rts_cranelift::observe`'s question and its own piece of work. A
/// trace that names the functions is what turns "something threw" into "this
/// path threw", which is the difference the report exists for.
///
/// A native has no name here, so it does not appear: what a program can act on
/// is its own frames.
/// The names on the call stack, innermost first.
///
/// # Why a host surface needs the frames as DATA
///
/// Because `util.getCallSite()` answers objects a program reads fields off,
/// where [`stack_text`] answers the rendered `at …` block an `Error` carries.
/// One list, two shapes: this is the same `context.callees` walk, exposed
/// rather than re-derived, for the reason [`stack_text`] records — a second
/// record of one fact disagrees the first time either forgets to pop.
///
/// # Why an unnamed frame is KEPT here and dropped from the printed trace
///
/// Because the two are read differently. A trace is read by a person, and a
/// line saying `at ` is worse than no line — which is why [`stack_text`] filters
/// them. This list is read by INDEX: `util.getCallSite()[1]` is how Node's own
/// test harness asks for "my caller's caller", and dropping a frame silently
/// shifts every index past it onto the wrong function.
///
/// That was not hypothetical. A module's top-level body is an unnamed function,
/// so a call made from there produced a one-element list, `[1]` was `undefined`,
/// and 18 files of Node's suite died reading a property of it — the shape of
/// bug an index-addressed list has and a printed one does not.
///
/// An unnamed frame contributes the empty string, which is what
/// `(function(){}).name` answers and therefore what a program comparing names
/// already handles.
///
/// **What it does not carry is a POSITION**, and the caller must say so rather
/// than fill one in. The machine records a source position per instruction and
/// nothing maps an address back to one at run time — `rts_cranelift::observe`'s
/// question. A zero here would be a line number a program could act on.
pub fn call_frames() -> Vec<String> {
    with_current(|context| {
        context
            .callees
            .iter()
            .rev()
            .map(|callee| {
                let named = crate::value::Value(*callee)
                    .as_slot()
                    .and_then(|cell| context.callable_at(cell))
                    .and_then(|(code, _)| {
                        context
                            .function_names
                            .iter()
                            .find(|(at, _, _, _)| *at == code)
                            .map(|(_, name, _, _)| name.clone())
                    });
                named.unwrap_or_default()
            })
            .collect()
    })
}

pub(in crate::entry) fn stack_text(context: &Context) -> String {
    let mut lines = String::new();
    for callee in context.callees.iter().rev() {
        let Some(cell) = crate::value::Value(*callee).as_slot() else {
            continue;
        };
        let Some((code, _)) = context.callable_at(cell) else {
            continue;
        };
        // An unnamed function contributes no line: a label a program cannot be
        // searched for is worse than a gap in the trace, which is why this
        // filter was the table's membership rule until `f.length` needed the
        // unnamed ones in it too.
        if let Some((_, name, _, _)) = context
            .function_names
            .iter()
            .find(|(at, name, _, _)| *at == code && !name.is_empty())
        {
            lines.push_str("\n    at ");
            lines.push_str(name);
        }
    }
    lines
}
