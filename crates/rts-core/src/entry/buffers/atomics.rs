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
//! The specification allows every integer typed array except `Uint8Clamped`.
//! `Float32Array`/`Float64Array`, `Uint8ClampedArray` and the two bigint kinds
//! are refused. Refusing answers `undefined` rather than raising, the same
//! tolerance every other refusal in this layer settles on: a native here cannot
//! raise a catchable error except at the two sites
//! `crates/rts-core/README.md` names, and this is not one of them.

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
            Some((held, at)) => Value::from_f64(read(context, &held, at)).bits(),
            None => objects::undefined_of(context),
        })
    }

    /// `Atomics.store(view, index, value)` — answers the value stored.
    fn store(view: u64, index: f64, value: f64) -> u64 {
        with_current(|context| match cell(context, view, index) {
            Some((held, at)) => {
                write(context, &held, at, value);
                Value::from_f64(value).bits()
            }
            None => objects::undefined_of(context),
        })
    }

    /// `Atomics.add(view, index, value)` — answers the value BEFORE the add.
    fn add(view: u64, index: f64, value: f64) -> u64 {
        rmw(view, index, value, |prior, value| prior + value)
    }

    /// `Atomics.sub(view, index, value)`.
    fn sub(view: u64, index: f64, value: f64) -> u64 {
        rmw(view, index, value, |prior, value| prior - value)
    }

    /// `Atomics.and(view, index, value)` — bitwise, over the wrapped integer.
    fn and(view: u64, index: f64, value: f64) -> u64 {
        rmw_bits(view, index, value, |prior, value| prior & value)
    }

    /// `Atomics.or(view, index, value)`.
    fn or(view: u64, index: f64, value: f64) -> u64 {
        rmw_bits(view, index, value, |prior, value| prior | value)
    }

    /// `Atomics.xor(view, index, value)`.
    fn xor(view: u64, index: f64, value: f64) -> u64 {
        rmw_bits(view, index, value, |prior, value| prior ^ value)
    }

    /// `Atomics.exchange(view, index, value)` — answers the prior value, stores
    /// the new one unconditionally.
    fn exchange(view: u64, index: f64, value: f64) -> u64 {
        rmw(view, index, value, |_prior, value| value)
    }

    /// `Atomics.compareExchange(view, index, expected, replacement)` — answers
    /// the value that was there; stores `replacement` only when it equalled
    /// `expected`.
    fn compare_exchange(view: u64, index: f64, expected: f64, replacement: f64) -> u64 {
        with_current(|context| match cell(context, view, index) {
            Some((held, at)) => {
                let prior = read(context, &held, at);
                if prior == expected {
                    write(context, &held, at, replacement);
                }
                Value::from_f64(prior).bits()
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
}

/// A read-modify-write over the element as a plain number: reads the prior
/// value, applies `apply(prior, operand)`, writes the result, answers the
/// prior value — the shape `add`/`sub`/`exchange` share.
fn rmw(view: u64, index: f64, operand: f64, apply: impl FnOnce(f64, f64) -> f64) -> u64 {
    with_current(|context| match cell(context, view, index) {
        Some((held, at)) => {
            let prior = read(context, &held, at);
            write(context, &held, at, apply(prior, operand));
            Value::from_f64(prior).bits()
        }
        None => objects::undefined_of(context),
    })
}

/// The same shape, for a bitwise operation: `f64` has no `&`/`|`/`^`, so both
/// operands cross through the wrapped 64-bit integer first — truncated the way
/// every element store already truncates, via `f64::trunc` rather than a
/// re-derivation of the element codec's rounding.
fn rmw_bits(view: u64, index: f64, operand: f64, apply: impl FnOnce(i64, i64) -> i64) -> u64 {
    with_current(|context| match cell(context, view, index) {
        Some((held, at)) => {
            let prior = read(context, &held, at);
            let result = apply(prior.trunc() as i64, operand.trunc() as i64) as f64;
            write(context, &held, at, result);
            Value::from_f64(prior).bits()
        }
        None => objects::undefined_of(context),
    })
}

/// The view and element index a call named, if both are valid: `view` names an
/// integer-kind typed array — not a bigint kind, not a float kind, not the
/// clamping one — and `index` is in range.
fn cell(context: &super::Context, view: u64, index: f64) -> Option<(View, usize)> {
    let held = super::view_of(context, view)?;
    if held.kind.is_bigint()
        || matches!(held.kind, Kind::Float32 | Kind::Float64 | Kind::Uint8Clamped | Kind::Raw)
    {
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

/// The element, as a number.
fn read(context: &super::Context, view: &View, at: usize) -> f64 {
    window(context, view)
        .and_then(|bytes| super::element::read(bytes, at * view.kind.size(), view.kind, true))
        .unwrap_or(0.0)
}

/// Writes the element.
fn write(context: &mut super::Context, view: &View, at: usize, value: f64) {
    if let Some(bytes) = window_mut(context, view) {
        super::element::write(bytes, at * view.kind.size(), view.kind, value, true);
    }
}
