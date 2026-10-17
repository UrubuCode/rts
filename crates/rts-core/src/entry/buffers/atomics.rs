//! `Atomics` — read-modify-write over a typed-array view, single-threaded.
//!
//! # Why "atomic" is honest here with no lock
//!
//! This engine has one OS thread. An operation that reads an element, computes,
//! and writes it back is only unsafe to call "atomic" if something else could
//! run between the read and the write — another thread, or a signal handler —
//! and neither exists here. So every operation below is a plain
//! read-then-write, and it is observably identical to what `Atomics` answers in
//! an engine that IS multi-threaded, under the one condition
//! [`super::shared_array_buffer`] is honest about too: nothing else in this
//! process is racing it, because nothing else runs at the same time.
//!
//! # Which kinds are accepted
//!
//! The specification allows every integer typed array except `Uint8Clamped`,
//! the bigint kinds included. `Float32Array`/`Float64Array` and
//! `Uint8ClampedArray` are refused. Refusing answers `undefined` rather than
//! raising, the same tolerance every other refusal in this layer settles on.
//!
//! `notify`, `wait` and `waitAsync` are stricter: they raise, as the
//! specification does, because a program that reaches them is asking about
//! another agent and a quiet `undefined` would read as an answer.
//!
//! # What `wait` is, with no second agent
//!
//! Nothing can write the cell or call `notify` while the one thread sleeps, so
//! `wait` answers `"not-equal"` when the cell already differs and otherwise
//! sleeps out the timeout and answers `"timed-out"`. An infinite timeout sleeps
//! forever, which is what the same call does on the main thread of Node.
//! `notify` therefore wakes nobody and answers `0`.
//!
//! `waitAsync` is the one place a waiter and a notifier can share a thread. A
//! finite wait is settled `"timed-out"` at once — it is not registered, so a
//! `notify` inside that window does not wake it (it would answer `"ok"` in
//! Node); an infinite wait is a promise that stays pending.

use super::element::Kind;
use super::{View, with_current, window, window_mut};
use crate::entry::objects;
use crate::value::Value;

/// `Atomics`.
#[rtse::class("Atomics", namespace, tag)]
impl Atomics {
    /// `Atomics.load(view, index)`.
    fn load(view: u64, index: f64) -> u64 {
        with_current(|context| match cell(context, view, index) {
            Some((held, at)) => read_value(context, &held, at),
            None => objects::undefined_of(context),
        })
    }

    /// `Atomics.store(view, index, value)` — answers the value stored.
    fn store(view: u64, index: f64, value: u64) -> u64 {
        with_current(|context| match cell(context, view, index) {
            Some((held, at)) => match operand(context, &held, value) {
                Some(word) => {
                    write_word(context, &held, at, word);
                    match held.kind.is_bigint() {
                        true => value,
                        false => Value::from_f64(super::super::class_support::to_number(value)).bits(),
                    }
                }
                None => objects::undefined_of(context),
            },
            None => objects::undefined_of(context),
        })
    }

    /// `Atomics.add(view, index, value)` — answers the value BEFORE the add.
    fn add(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |prior, value| prior.wrapping_add(value))
    }

    /// `Atomics.sub(view, index, value)`.
    fn sub(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |prior, value| prior.wrapping_sub(value))
    }

    /// `Atomics.and(view, index, value)` — bitwise, over the wrapped integer.
    fn and(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |prior, value| prior & value)
    }

    /// `Atomics.or(view, index, value)`.
    fn or(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |prior, value| prior | value)
    }

    /// `Atomics.xor(view, index, value)`.
    fn xor(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |prior, value| prior ^ value)
    }

    /// `Atomics.exchange(view, index, value)` — answers the prior value, stores
    /// the new one unconditionally.
    fn exchange(view: u64, index: f64, value: u64) -> u64 {
        rmw(view, index, value, |_prior, value| value)
    }

    /// `Atomics.compareExchange(view, index, expected, replacement)` — answers
    /// the value that was there; stores `replacement` only when it equalled
    /// `expected`.
    fn compare_exchange(view: u64, index: f64, expected: u64, replacement: u64) -> u64 {
        with_current(|context| match cell(context, view, index) {
            Some((held, at)) => {
                let (Some(expected), Some(replacement)) =
                    (operand(context, &held, expected), operand(context, &held, replacement))
                else {
                    return objects::undefined_of(context);
                };
                let prior = read_word(context, &held, at);
                // Compared as the stored width, not as the number the program
                // wrote: `compareExchange(i8, 0, 255, …)` matches a cell holding -1.
                if prior == narrowed(expected, held.kind) {
                    write_word(context, &held, at, replacement);
                }
                answer(context, &held, prior)
            }
            None => objects::undefined_of(context),
        })
    }

    /// `Atomics.isLockFree(size)`.
    ///
    /// # Why the answer is a width and not `true`
    ///
    /// The question is whether an atomic operation at that byte width is done
    /// without taking a lock, and it is asked so a program can choose a layout.
    /// Answering `true` for every size would be the useful-looking lie: a
    /// program picking eight-byte elements on the strength of it would get the
    /// slow path on any target where that width is emulated.
    ///
    /// So the answer is the set every implementation this engine can run on
    /// agrees about — 1, 2, 4 and 8 bytes, which is what a 64-bit target does
    /// natively — and `false` for the sizes that are not element widths at all.
    /// The operations above run on one thread and compute exactly what their
    /// names say, which is the same argument `rts:atomic`'s own module makes for
    /// existing at all.
    fn is_lock_free(size: f64) -> bool {
        matches!(size, 1.0 | 2.0 | 4.0 | 8.0)
    }

    /// `Atomics.notify(view, index, count)` — how many waiters it woke.
    ///
    /// Always `0`: the only agent is the caller, and it is not waiting. The
    /// array and the index are still checked, because a program that passes a
    /// bad one is wrong in every engine.
    fn notify(view: u64, index: u64, _count: u64) -> u64 {
        match waitable(view, index, false) {
            Some(_) => Value::from_f64(0.0).bits(),
            None => super::undefined(),
        }
    }

    /// `Atomics.wait(view, index, value, timeout)`.
    ///
    /// See the module doc for why this is a sleep and not a wait.
    fn wait(view: u64, index: u64, value: u64, timeout: u64) -> u64 {
        let Some((held, at)) = waitable(view, index, true) else {
            return super::undefined();
        };
        let expected = with_current(|context| operand(context, &held, value));
        let Some(expected) = expected else {
            conversion_error(&held, value);
            return super::undefined();
        };
        let wait = milliseconds(timeout);
        let differs = with_current(|context| read_word(context, &held, at) != narrowed(expected, held.kind));
        if differs {
            return text("not-equal");
        }
        sleep(wait);
        text("timed-out")
    }

    /// `Atomics.waitAsync(view, index, value, timeout)` — `{ async, value }`.
    fn wait_async(view: u64, index: u64, value: u64, timeout: u64) -> u64 {
        let Some((held, at)) = waitable(view, index, true) else {
            return super::undefined();
        };
        let expected = with_current(|context| operand(context, &held, value));
        let Some(expected) = expected else {
            conversion_error(&held, value);
            return super::undefined();
        };
        let wait = milliseconds(timeout);
        let differs = with_current(|context| read_word(context, &held, at) != narrowed(expected, held.kind));
        // Settled at once: `not-equal`, and a zero timeout, which Node also
        // answers synchronously. Both are `async: false` with the string itself.
        if differs || wait == Some(0.0) {
            let outcome = text(if differs { "not-equal" } else { "timed-out" });
            return outcome_object(false, outcome);
        }
        // `promise_new` rather than `resolved_with`: it registers `Promise`
        // first, and a program that never wrote the word would otherwise get a
        // promise with no prototype and so no `.then`.
        let promise = crate::entry::promise::promise_new();
        // Nobody can notify an infinite wait, so it is never settled.
        if wait.is_some() {
            crate::entry::promise::promise_settle(promise, text("timed-out"), 0);
        }
        outcome_object(true, promise)
    }
}

/// `{ async, value }`, the answer of `waitAsync`.
fn outcome_object(is_async: bool, value: u64) -> u64 {
    with_current(|context| {
        let Some(holder) = crate::entry::native::plain(context) else {
            return objects::undefined_of(context);
        };
        let key = context.well_known("async");
        objects::put(context, holder, key, Value::from_bool(is_async).bits());
        let key = context.well_known("value");
        objects::put(context, holder, key, value);
        Value::from_slot(holder).bits()
    })
}

/// A string value.
fn text(spelled: &str) -> u64 {
    with_current(|context| crate::entry::modules::make_string(context, spelled))
}

/// The timeout in milliseconds; `None` is forever.
///
/// An absent timeout and `NaN` are both infinite, a negative one is zero — the
/// specification's own reading, not a tolerance.
fn milliseconds(timeout: u64) -> Option<f64> {
    if timeout == super::undefined() {
        return None;
    }
    let asked = super::super::class_support::to_number(timeout);
    if asked.is_nan() || asked == f64::INFINITY {
        return None;
    }
    Some(asked.max(0.0))
}

/// Blocks the thread for `wait` milliseconds, or for good.
///
/// Forever is a real block, as it is in Node on the main thread: with no other
/// agent nothing can end it.
fn sleep(wait: Option<f64>) {
    match wait {
        Some(ms) => std::thread::sleep(std::time::Duration::from_secs_f64(ms / 1000.0)),
        None => loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        },
    }
}

/// The view and element index `notify`/`wait` were asked about, or raises.
///
/// `Int32Array` and `BigInt64Array` only. `wait` additionally wants the bytes
/// shared, `notify` on an unshared one is simply `0` and so is allowed through.
fn waitable(view: u64, index: u64, shared: bool) -> Option<(View, usize)> {
    let asked = super::super::class_support::to_number(index);
    // Raises after the borrow is released: `throw` takes the context itself.
    let found = with_current(|context| {
        let Some(held) = super::view_of(context, view) else {
            // An array is the other common mistake and has no view to name it.
            let name = match crate::value::Value(view).as_slot().and_then(|cell| context.elements_at(cell)) {
                Some(_) => "[object Array]",
                None => described(None),
            };
            return Err(format!("{name} is not an int32 or BigInt64 typed array."));
        };
        if !matches!(held.kind, Kind::Int32 | Kind::BigInt64) {
            return Err(format!("{} is not an int32 or BigInt64 typed array.", described(Some(held.kind))));
        }
        if shared && !is_shared(context, &held) {
            return Err(format!("{} is not a shared typed array.", described(Some(held.kind))));
        }
        let at = if asked.is_nan() { 0.0 } else { asked.trunc() };
        if at < 0.0 || at >= held.count() as f64 {
            return Ok(Err(()));
        }
        Ok(Ok((held, at as usize)))
    });
    match found {
        Err(message) => {
            crate::entry::throw::type_error(&message);
            None
        }
        Ok(Err(())) => {
            crate::entry::throw::range_error("Invalid atomic access index");
            None
        }
        Ok(Ok(found)) => Some(found),
    }
}

/// `[object Int32Array]`, the way the engine's own message spells a view.
fn described(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::Int8) => "[object Int8Array]",
        Some(Kind::Uint8) => "[object Uint8Array]",
        Some(Kind::Uint8Clamped) => "[object Uint8ClampedArray]",
        Some(Kind::Int16) => "[object Int16Array]",
        Some(Kind::Uint16) => "[object Uint16Array]",
        Some(Kind::Int32) => "[object Int32Array]",
        Some(Kind::Uint32) => "[object Uint32Array]",
        Some(Kind::Float32) => "[object Float32Array]",
        Some(Kind::Float64) => "[object Float64Array]",
        Some(Kind::BigInt64) => "[object BigInt64Array]",
        Some(Kind::BigUint64) => "[object BigUint64Array]",
        _ => "[object Object]",
    }
}

/// Whether the view's bytes belong to a `SharedArrayBuffer`.
///
/// Told by the buffer's prototype: the two buffers share a representation, and
/// the prototype is the one thing `shared_array_buffer`'s constructor sets
/// differently.
fn is_shared(context: &super::Context, view: &View) -> bool {
    let Some(shared) = super::super::class_support::prototype(context, "SharedArrayBuffer") else {
        return false;
    };
    context.prototype_at(view.buffer) == Some(shared)
}

/// Raises the `TypeError` for a value the element kind refuses.
fn conversion_error(view: &View, value: u64) {
    let _ = value;
    match view.kind.is_bigint() {
        true => crate::entry::throw::type_error("Cannot convert value to a BigInt"),
        false => crate::entry::throw::type_error("Cannot convert a BigInt value to a number"),
    }
}

/// A read-modify-write over the element's stored word: reads the prior value,
/// applies `apply(prior, operand)`, writes the result, answers the prior value
/// — the shape every operation but `compareExchange` shares.
///
/// Done over the raw word so one body serves the numeric kinds and the bigint
/// ones: `add` wraps at the element's width on the way in (`narrowed`), which is
/// the same truncation a store applies.
fn rmw(view: u64, index: f64, operand_value: u64, apply: impl FnOnce(u64, u64) -> u64) -> u64 {
    with_current(|context| match cell(context, view, index) {
        Some((held, at)) => {
            let Some(word) = operand(context, &held, operand_value) else {
                return objects::undefined_of(context);
            };
            let prior = read_word(context, &held, at);
            write_word(context, &held, at, apply(prior, word));
            answer(context, &held, prior)
        }
        None => objects::undefined_of(context),
    })
}

/// The view and element index a call named, if both are valid: `view` names an
/// integer-kind typed array — not a float kind, not the clamping one — and
/// `index` is in range.
fn cell(context: &super::Context, view: u64, index: f64) -> Option<(View, usize)> {
    let held = super::view_of(context, view)?;
    if matches!(held.kind, Kind::Float32 | Kind::Float64 | Kind::Uint8Clamped | Kind::Raw) {
        return None;
    }
    if !index.is_finite() || index < 0.0 {
        return None;
    }
    let at = index.trunc() as usize;
    if at >= held.count() {
        return None;
    }
    Some((held, at))
}

/// The word a program's operand becomes for this element kind.
///
/// `None` when the kind refuses it: a number for a bigint element, a bigint for
/// a numeric one. A number is truncated the way every element store truncates
/// it, via `f64::trunc`, and then taken modulo the width by `narrowed`.
fn operand(context: &super::Context, view: &View, value: u64) -> Option<u64> {
    if view.kind.is_bigint() {
        return super::bigint_word(context, value, view.kind);
    }
    if super::super::bigints::digits_of(context, value).is_some() {
        return None;
    }
    let number = super::super::class_support::to_number(value);
    let whole = if number.is_finite() { number.trunc() } else { 0.0 };
    // `as i64` saturates; the width mask in `narrowed` then wraps, which agrees
    // with modulo for every value an element can hold.
    Some(whole as i64 as u64)
}

/// A word reduced to the stored width, so two spellings of one element compare equal.
fn narrowed(word: u64, kind: Kind) -> u64 {
    match kind.size() {
        1 => word & 0xff,
        2 => word & 0xffff,
        4 => word & 0xffff_ffff,
        _ => word,
    }
}

/// The element's stored word, zero-extended.
fn read_word(context: &super::Context, view: &View, at: usize) -> u64 {
    window(context, view)
        .and_then(|bytes| super::element::word_at(bytes, at * view.kind.size(), view.kind, true))
        .unwrap_or(0)
}

/// Writes the element's word (the width truncates it).
fn write_word(context: &mut super::Context, view: &View, at: usize, word: u64) {
    if let Some(bytes) = window_mut(context, view) {
        super::element::write_word(bytes, at * view.kind.size(), view.kind, narrowed(word, view.kind), true);
    }
}

/// A stored word as the value the program sees: signed or unsigned per kind.
fn answer(context: &mut super::Context, view: &View, word: u64) -> u64 {
    if view.kind.is_bigint() {
        return super::bigint_value(context, word, view.kind);
    }
    let number = match view.kind {
        Kind::Int8 => f64::from(word as u8 as i8),
        Kind::Int16 => f64::from(word as u16 as i16),
        Kind::Int32 => f64::from(word as u32 as i32),
        _ => word as f64,
    };
    Value::from_f64(number).bits()
}

/// The element as the value the program sees.
fn read_value(context: &mut super::Context, view: &View, at: usize) -> u64 {
    let word = read_word(context, view, at);
    answer(context, view, word)
}
