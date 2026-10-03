//! The `Timeout` and `Immediate` objects `setTimeout`/`setInterval`/
//! `setImmediate` answer — the shape a program reads when it writes
//! `setTimeout(fn, 0).unref()`.
//!
//! # Why this exists now, when the number was defensible
//!
//! [`super`]'s module doc used to say the handle is a number because "with no
//! event loop, *keep the process alive* and *refresh the deadline* have nothing
//! to act on". The reason was true when written and both halves of it have since
//! stopped being: `entry::loops` is a loop, a source that answers
//! [`entry::Pending::In`] is what holds a program open, and one that answers
//! `Blocked` is pumped without holding it. So `unref()` has an exact meaning
//! here — *stop contributing an `In`* — and `refresh()` has one too, because the
//! deadline is a field in a table this crate owns.
//!
//! What forced it is a real program rather than a tidiness argument:
//! `@whiskeysockets/baileys` fails with `(intermediate value).unref is not a
//! function` on its first socket, and `.unref()` is the single most common thing
//! written about a timer handle in published JavaScript.
//!
//! # Where the state lives, and why some of it is on the OBJECT
//!
//! Split deliberately, and the split follows what Node makes observable:
//!
//! - The **schedule** — deadline, period, phase — stays in [`super::TIMERS`],
//!   because that is what [`super::pump`] and [`super::source`] read and a
//!   second copy of a deadline is two answers to "when does this fire".
//! - The **ref flag**, the **callback**, the **argument**, the **delay** and
//!   whether the handle was destroyed live on the JS object, as `@@`-prefixed
//!   own properties. Measured in Node 22: `hasRef()` still answers `true` after
//!   the timer has fired, been cleared or been closed — it reports the FLAG, not
//!   the liveness — and `refresh()` re-arms a handle whose callback has already
//!   run. Both need state that outlives the table entry, so neither can live in
//!   the table.
//!
//! The alternative was a second native table keyed by id, holding the callback
//! until the handle died. It was rejected on `rts-core`'s README rule 10: a
//! native table that names a cell is a table the collector has to be told
//! about, and this crate has no way to tell it. An own property of an ordinary
//! object is traced by the ordinary edge walk, with nothing new to register — so
//! the design that is easier to write is also the one that cannot lose a root.
//!
//! The `@@` prefix is not decoration: `rts-core`'s `symbol.rs` filters those
//! keys out of enumeration, so `Object.keys(timeout)` stays empty and
//! `JSON.stringify` of one does not print this module's bookkeeping.
//!
//! # Divergences that remain, by name
//!
//! - `Timeout.prototype[Symbol.toPrimitive].name` reads `"@@toPrimitive"` rather
//!   than `"[Symbol.toPrimitive]"`. The method is installed through the ordinary
//!   name table, which names a method by the key it stores it under;
//!   `rts-core`'s `date/hint.rs` is the one place that separates the two, with a
//!   private `put` a host cannot reach.
//! - `new (t.constructor)()` answers a bare `Timeout`-shaped object with no
//!   schedule behind it. Node's internal constructor takes the callback and the
//!   delay; nothing reaches it by name from a program, so the back link exists
//!   for `t.constructor.name` — which is what a program actually reads — and not
//!   as a way to make one.
//! - `_idleTimeout`, `_onTimeout`, `_destroyed` and the rest of Node's
//!   underscore-prefixed internals are absent. They are documented as internal
//!   and a program reading them is reading Node's implementation, not its API.

use rts_core::entry::{self, Context, Provided};

/// The raw table id this handle names. A number rather than the handle's
/// identity because [`super::forget`] and [`super::set_refed`] key by it, and
/// because it is what `Symbol.toPrimitive` has to answer.
const ID: &str = "@@#timer_id";
/// Whether `ref()` or `unref()` was called last. See the module doc for why this
/// is not read off the table.
const REFED: &str = "@@#timer_refed";
/// Whether `clearTimeout`/`close()`/`Symbol.dispose` destroyed this handle.
///
/// Distinct from "not in the table": a timer that FIRED is also absent from the
/// table, and Node's `refresh()` re-arms that one while refusing a cleared one.
/// Without this flag the two are indistinguishable and `refresh` would have to
/// guess which.
const DEAD: &str = "@@#timer_dead";
/// What `refresh()` re-schedules.
const CALLBACK: &str = "@@#timer_callback";
/// The single forwarded argument, kept beside the callback for the same reason.
const ARG: &str = "@@#timer_arg";
/// The delay in milliseconds, already clamped — `refresh()` is defined as
/// "start the original delay again", so the original is what is kept.
const DELAY: &str = "@@#timer_delay";
/// Whether this came from `setInterval`, which `refresh()` has to preserve.
const PERIODIC: &str = "@@#timer_periodic";

/// `Timeout.prototype`'s methods, in the order Node's own prototype lists them.
///
/// `@@dispose` and `@@toPrimitive` sit in this table rather than being installed
/// separately because a symbol key in this engine IS an interned name with a
/// reserved prefix (`rts-core`'s `symbol.rs`), so the ordinary installer already
/// does the right thing — which is also the whole reason `put_member(…,
/// "@@toStringTag", …)` works elsewhere in this crate.
const TIMEOUT_METHODS: &[(&str, Provided)] = &[
    ("refresh", refresh),
    ("unref", unref),
    ("ref", add_ref),
    ("hasRef", has_ref),
    ("close", close),
    ("@@dispose", dispose),
    ("@@toPrimitive", to_primitive),
];

/// `Immediate.prototype`'s methods — a relative and not the same class.
///
/// Measured in Node 22: an `Immediate` carries `ref`/`unref`/`hasRef` and
/// `Symbol.dispose` and **no** `Symbol.toPrimitive`, so `Number(immediate)` is
/// `NaN` there. Giving it one would be a convenience Node does not have, and a
/// program branching on `Number.isNaN(Number(h))` to tell the two apart would
/// read the wrong answer here.
const IMMEDIATE_METHODS: &[(&str, Provided)] = &[
    ("unref", unref),
    ("ref", add_ref),
    ("hasRef", has_ref),
    ("@@dispose", dispose),
];

/// The handle `setTimeout`/`setInterval` answer.
pub(super) fn timeout(
    context: &mut Context,
    id: u64,
    callback: u64,
    arg: u64,
    delay_ms: u64,
    periodic: bool,
) -> u64 {
    let prototype = prototype_of(context, "Timeout", TIMEOUT_METHODS);
    let instance = entry::make_instance(context, prototype);
    describe(context, instance, id, callback, arg, delay_ms, periodic);
    instance
}

/// The handle `setImmediate` answers.
pub(super) fn immediate(context: &mut Context, id: u64, callback: u64, arg: u64) -> u64 {
    let prototype = prototype_of(context, "Immediate", IMMEDIATE_METHODS);
    let instance = entry::make_instance(context, prototype);
    describe(context, instance, id, callback, arg, 0, false);
    instance
}

/// Registers both classes — called ONCE per context, from
/// [`super::surface::namespace`].
///
/// # Why not from `timeout()`, where the prototype is needed
///
/// Because `declare_host_class` has to run exactly once and neither of the two
/// ways of deciding that from the scheduling path works. `make_prototype`
/// memoizes the prototype but not the constructor, so calling it
/// unconditionally mints a fresh callable per `setTimeout` — one dead cell per
/// timer on this module's hottest path, and `t1.constructor !== t2.constructor`,
/// which programs do test. Guarding on `get_member(prototype, "constructor")`
/// was tried and MEASURED wrong: it answered `Object`'s constructor rather than
/// nothing, so the guard never fired and `t.constructor.name` read `"Object"` —
/// 23 of the fixture's 26 assertions passed and the three about the class name
/// did not.
///
/// The namespace is built once per context, before any global can be read out of
/// it, so that is where "once" already lives. Nothing here is per timer.
pub(super) fn declare(context: &mut Context) {
    class(context, "Timeout", TIMEOUT_METHODS);
    class(context, "Immediate", IMMEDIATE_METHODS);
}

/// One class: the memoized prototype, plus the constructor and the back link.
fn class(context: &mut Context, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let prototype = entry::make_prototype(context, name, methods);
    let constructor = entry::make_callable(context, construct);
    entry::put_member(context, constructor, "prototype", prototype);
    entry::declare_host_class(context, constructor, prototype, name, 0);
    prototype
}

/// The prototype of one of the two classes, as [`declare`] left it.
fn prototype_of(context: &mut Context, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    entry::make_prototype(context, name, methods)
}

/// Writes the seven hidden properties a handle carries.
fn describe(
    context: &mut Context,
    instance: u64,
    id: u64,
    callback: u64,
    arg: u64,
    delay_ms: u64,
    periodic: bool,
) {
    let id_value = entry::make_number(id as f64);
    entry::put_member(context, instance, ID, id_value);
    let refed = entry::boolean_value(true);
    entry::put_member(context, instance, REFED, refed);
    let alive = entry::boolean_value(false);
    entry::put_member(context, instance, DEAD, alive);
    entry::put_member(context, instance, CALLBACK, callback);
    entry::put_member(context, instance, ARG, arg);
    let delay = entry::make_number(delay_ms as f64);
    entry::put_member(context, instance, DELAY, delay);
    let repeats = entry::boolean_value(periodic);
    entry::put_member(context, instance, PERIODIC, repeats);
}

/// `prototype.constructor`'s body — see the module doc's third divergence for
/// why it schedules nothing.
extern "C" fn construct(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

/// This handle's raw table id, or `None` for anything that is not one.
///
/// Public to the module because [`super::cancel`] has to accept a handle AND the
/// number it coerces to: Node takes either, and a program that moved a handle
/// across a boundary as a number is the reason `Symbol.toPrimitive` exists.
pub(super) fn id_of(context: &mut Context, value: u64) -> Option<u64> {
    entry::number_of(entry::get_member(context, value, ID)).map(|number| number as u64)
}

/// A boolean hidden property, read as Rust.
fn flag(context: &mut Context, this: u64, name: &str) -> bool {
    entry::get_member(context, this, name) == entry::boolean_value(true)
}

/// `timeout.unref()` — stop holding the program open. Answers the handle, which
/// is what makes `const t = setTimeout(f, 0).unref()` the idiom it is.
extern "C" fn unref(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    set_ref(this, false);
    this
}

/// `timeout.ref()` — the other half.
extern "C" fn add_ref(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    set_ref(this, true);
    this
}

/// Writes the flag on the handle and pushes it into the table entry, when there
/// still is one.
///
/// Both, and not one derived from the other: the flag outlives the entry (Node's
/// `hasRef()` answers after the timer fired) while [`super::source`] can only
/// read the entry.
fn set_ref(this: u64, refed: bool) {
    let id = entry::with_runtime(|context| {
        let held = entry::boolean_value(refed);
        entry::put_member(context, this, REFED, held);
        id_of(context, this)
    });
    if let Some(id) = id {
        super::set_refed(id, refed);
    }
}

/// `timeout.hasRef()`.
extern "C" fn has_ref(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(entry::with_runtime(|context| flag(context, this, REFED)))
}

/// `timeout.refresh()` — start the original delay again, as if the timer had
/// just been scheduled.
///
/// Re-registers under the SAME id, because Node keeps `Number(t)` stable across
/// a refresh and a program that cancelled by the primitive would otherwise hold
/// a number naming nothing.
///
/// A destroyed handle is refused and answers itself unchanged, which is Node's
/// behaviour measured rather than assumed: `clearTimeout(t); t.refresh()` does
/// not fire, while a refresh after the callback has already run does.
extern "C" fn refresh(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let plan = entry::with_runtime(|context| {
        if flag(context, this, DEAD) {
            return None;
        }
        let id = id_of(context, this)?;
        let callback = entry::get_member(context, this, CALLBACK);
        let arg = entry::get_member(context, this, ARG);
        let delay = entry::number_of(entry::get_member(context, this, DELAY)).unwrap_or(1.0);
        let periodic = flag(context, this, PERIODIC);
        let refed = flag(context, this, REFED);
        Some((id, callback, arg, delay.max(1.0) as u64, periodic, refed))
    });
    if let Some((id, callback, arg, delay_ms, periodic, refed)) = plan {
        super::rearm(id, callback, arg, delay_ms, periodic, refed);
    }
    this
}

/// `timeout.close()` — cancel, answering the handle. Node's own spelling for
/// what `clearTimeout` does, kept because `server.close()`-shaped code reaches
/// for it.
extern "C" fn close(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    destroy(this);
    this
}

/// `timeout[Symbol.dispose]()` — what `using t = setTimeout(…)` calls. Answers
/// `undefined`, because that is what the disposal protocol ignores.
extern "C" fn dispose(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    destroy(this);
    entry::undefined_value()
}

/// Marks a handle destroyed and drops its table entry — the one path
/// `clearTimeout`, `close()` and `Symbol.dispose` share, so the three cannot
/// come to disagree about what "cancelled" means.
pub(super) fn destroy(this: u64) {
    let id = entry::with_runtime(|context| {
        let dead = entry::boolean_value(true);
        entry::put_member(context, this, DEAD, dead);
        id_of(context, this)
    });
    if let Some(id) = id {
        super::forget(id);
    }
}

/// `timeout[Symbol.toPrimitive]()` — the id, which is what `clearTimeout` of a
/// number cancels and what a program printing a handle sees.
///
/// The hint is ignored on purpose: Node answers the same number for `"number"`,
/// `"string"` and `"default"` alike (`String(t)` is `"2"`, not `"[object
/// Timeout]"`), so branching on it would invent a distinction the thing being
/// matched does not have.
extern "C" fn to_primitive(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| entry::get_member(context, this, ID))
}
