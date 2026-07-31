//! Generators and the lazy state machine — coroutines, which the Cranelift IR
//! cannot express. Clause (a) of the partition rule (`RTS_ORGANIZATION.md` §1).
//!
//! ## What is here and what is not
//!
//! `RTS_ORGANIZATION.md` N3 described moving `rts-std/src/collector/generator.rs`
//! down whole, "1210 lines, 38 `__rtsn_` symbols". Measured, that file was two
//! things at once and only one of them is machinery:
//!
//! - **here** — the eager generator protocol ([`eager`]), the lazy state machine
//!   ([`sm`]), the iterator helpers ([`iterhelp`]) and `yield*` delegation
//!   ([`delegate`]). Suspending and resuming a frame is precisely what the IR
//!   has no way to say;
//! - **still in `rts-std`** (`collector::generator`) — the ASYNC driver
//!   (`async_sm_*`, `agen_*`). It polls timers, enters a tokio worker guard and
//!   enqueues microtasks. That is backend, and moving it would have dragged five
//!   `rts-std` modules down with it.
//!
//! The dividing line is not "sync vs async" by name: it is that a state machine
//! is a data structure with a `step`, while DRIVING one to completion means
//! deciding when to run it, which is scheduling.
//!
//! ## The one place the two halves still meet
//!
//! An ASYNC generator's `.next()` returns a Promise, so `gen_sm_next` and
//! `gen_sm_drain` — both here — have to reach the async driver for that case.
//! Rather than name it, they call through [`AgenDriver`], which `rts-std`
//! installs at startup. Same shape and same reasoning as
//! [`super::root_sources`]: the lower layer owns the mechanism, the upper layer
//! contributes the part that needs a scheduler.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::heap::handles::{Entry, with_entry};

pub mod delegate;
pub mod eager;
pub mod iterhelp;
pub mod sm;

pub use delegate::*;
pub use eager::*;
pub use iterhelp::*;
pub use sm::*;

/// Sentinel `undefined` (i64::MIN+2) — convencao do codegen/INSPECT.
pub(crate) const UNDEFINED: i64 = i64::MIN + 2;
/// Sentinels de bool: MIN = false, MIN+1 = true (igual TPL_COERCE_AUTO).
pub(crate) const BOOL_FALSE: i64 = i64::MIN;
pub(crate) const BOOL_TRUE: i64 = i64::MIN + 1;

thread_local! {
    /// Cursor de iteracao por handle de Vec consumido via `.next()`.
    pub(crate) static GEN_CURSORS: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
    /// Valor de `return X` do generator por handle de Vec. Devolvido pelo
    /// primeiro `.next()` apos esgotar os yields (`{value:X, done:true}`).
    pub(crate) static GEN_RETS: RefCell<HashMap<u64, i64>> = RefCell::new(HashMap::new());
}

/// How to drive an ASYNC generator — supplied by `rts-std`, which owns the
/// scheduler this needs.
///
/// Both entries are plain `fn` pointers rather than a trait object so the whole
/// thing is a `Copy` value in a `OnceLock`, readable on the hot path with no
/// allocation and no lock.
#[derive(Clone, Copy)]
pub struct AgenDriver {
    /// `agen_next(h)` — advance an async generator one step, returning the
    /// handle of a Promise of `{value, done}`.
    pub next: fn(u64) -> u64,
    /// Block until a Promise settles and return its value. Rejection is reported
    /// through the pending-error slot ([`super::error`]), not the return value.
    pub await_settled: fn(u64) -> i64,
}

static AGEN_DRIVER: OnceLock<AgenDriver> = OnceLock::new();

/// Install the async-generator driver. Called once at startup; first writer wins.
pub fn set_agen_driver(d: AgenDriver) {
    let _ = AGEN_DRIVER.set(d);
}

/// The installed driver, or `None` when the runtime has not booted (unit tests,
/// compile-time execution). Callers treat `None` as "this generator is done" —
/// an async generator cannot make progress without a scheduler, and reporting
/// `done` is the only answer that neither blocks nor invents a value.
pub(crate) fn agen_driver() -> Option<AgenDriver> {
    AGEN_DRIVER.get().copied()
}

/// Is `h` an async generator (as opposed to a plain one)?
pub(crate) fn is_async_gen(h: u64) -> bool {
    with_entry(h, |e| matches!(e, Some(Entry::GenState(g)) if g.is_async_gen))
}

/// Aloca o objeto-resultado `{value, done}` como objeto SHAPED do motor novo
/// (Entry::Vec com shape-id no slot 0) — o modelo de objeto único, sem o
/// dicionário legacy Entry::Map. Slots guardam PolyValue WORDS.
pub fn make_result(value: i64, done: bool) -> u64 {
    use crate::heap::shapes::{alloc_shaped_object, bool_word, legacy_i64_to_word};
    alloc_shaped_object(
        &["value", "done"],
        &[legacy_i64_to_word(value) as i64, bool_word(done) as i64],
    )
}

/// Lê `(value_word, done)` de um resultado `{value, done}` shaped (o formato
/// que [`make_result`] produz). `None` quando o handle não é um shaped result.
pub fn read_result_parts(h: u64) -> Option<(i64, bool)> {
    use crate::heap::shapes::bool_word;
    with_entry(h, |e| match e {
        Some(Entry::Vec(slots)) if slots.len() == 3 => {
            Some((slots[1], slots[2] as u64 == bool_word(true)))
        }
        _ => None,
    })
}
