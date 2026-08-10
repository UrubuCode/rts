//! `pbkdf2`, `scrypt`, `hkdf` — the callback halves of [`super::sync`].
//!
//! # Why these are not just the sync function plus a call
//!
//! Because a Node program is allowed to rely on the callback arriving on a
//! LATER turn. `crypto.pbkdf2(…, cb); after();` runs `after` first in Node, and
//! a version that called `cb` inline would reorder every program that assumed
//! it. Correct bytes at the wrong time is still a wrong answer, just a subtler
//! one than the zeros this campaign removed.
//!
//! So the derivation runs synchronously — there is no thread pool this crate may
//! reach, and `crypto.md` §5.7 puts the shared tokio runtime in `rts-std`, which
//! `rts-node-rwk` must not depend on — and the DELIVERY is deferred to the next
//! pump of the event loop.
//!
//! # Reuse-check (`.claude/skills/reuse-check`)
//!
//! - `entry::declare_loop_source` / `entry::Pending` **already exist** and are
//!   what defers this. No second queue, no second clock, no thread.
//! - `node:timers`' `Deliver::Call` is the same shape one level up: a callback
//!   and its argument held in a module table until the loop asks. It is not
//!   reused directly because reaching into `super::super::super::timers`' private
//!   table would make `node:crypto` a second writer of `node:timers`' ids — and
//!   a KDF job has no deadline, no period and no `clearTimeout`, so three of the
//!   four fields would be dead. This table holds `Vec<u8>`, not a timer.
//! - Ids are not minted at all: a job is delivered once and dropped, and nothing
//!   in Node's API can cancel one. Nothing to number, so nothing to collide.
//!
//! # What is deferred, and what is not
//!
//! The COST is not. Node computes on its thread pool, so a long `scrypt` leaves
//! the main thread free; here it blocks the caller and only the callback moves.
//! Stated rather than discovered: `crypto.scrypt` with `N = 2^20` will stall a
//! server here where Node's would not. The alternative is a thread this crate
//! may not create, and the alternative to THAT is not offering the function —
//! which is worse, because the bytes are right and the timing is documented.
//!
//! # Argument errors are raised, not passed to the callback
//!
//! Node validates every KDF parameter SYNCHRONOUSLY and throws
//! (`ERR_INVALID_ARG_TYPE`, `ERR_OUT_OF_RANGE`,
//! `ERR_CRYPTO_INVALID_SCRYPT_PARAMS`) rather than delivering an `Error` as the
//! callback's first argument — there is no operational failure left for a KDF
//! once its parameters are good. So the `err` these deliver is always `null`,
//! and every refusal reaches the caller through the same raise the `*Sync` form
//! uses.

use std::sync::Mutex;
use std::time::Duration;

use rts_core_rwk::entry::{self, Context};

use super::args::{HkdfArguments, Pbkdf2Arguments, ScryptArguments, ScryptOptions};
use super::sync;
use super::super::util;

/// One derived key waiting for the next pump: the callback to invoke and the
/// bytes to hand it.
///
/// The callback is held as a raw `u64` across a turn, which is a root the
/// collector does not know about — the same exposure `node:timers`' own table
/// has for `Deliver::Call`, and named here rather than left to be rediscovered.
/// The BYTES are a `Vec<u8>` and not a `Buffer`, so the only value at risk is
/// the callback itself, and it is at risk for one pump.
struct Job {
    callback: u64,
    bytes: Vec<u8>,
}

/// Every job not yet delivered, oldest first.
///
/// A `Mutex<Vec<_>>` and not a thread-local: `crypto.md`'s callback forms have
/// no per-thread identity to key on, and a worker deriving a key wants its own
/// callback delivered on its own thread — which is what happens, because a
/// worker runs its own context and pumps its own sources. A poisoned lock
/// answers an empty queue rather than panicking inside a loop source.
static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());

/// Registers the delivery source with this thread's context.
///
/// Called from `super::super::namespace`, at install time, and never by the
/// host: `entry::declare_loop_source`'s own doc says the host naming sources is
/// what left four of six unpumped.
pub(crate) fn declare(context: &mut Context) {
    entry::declare_loop_source(context, "node:crypto/kdf", source);
}

/// Delivers every queued callback, then says whether more are outstanding.
///
/// The lock is released BEFORE any callback runs. A callback is user code that
/// will call back into `node:crypto` — including `crypto.pbkdf2` again — and
/// holding the lock across it would deadlock on the re-entrant push.
/// `entry::pump_sources` states the same rule one level up for the runtime
/// borrow.
fn source() -> entry::Pending {
    let due: Vec<Job> = match JOBS.lock() {
        Ok(mut held) => std::mem::take(&mut *held),
        Err(_) => Vec::new(),
    };
    let absent = entry::undefined_value();
    let no_error = entry::null_value();
    for job in due {
        let key = entry::with_runtime(|context| entry::make_buffer(context, &job.bytes));
        entry::call(job.callback, absent, no_error, key, absent, absent);
    }
    // `Pending::In(ZERO)` and not `Blocked`: a queued job MUST hold the program
    // open, or `crypto.pbkdf2(…, cb)` as a program's last statement would exit
    // before `cb` ran. `Blocked` is for a source waiting on the outside world,
    // which this never is — the work is already done.
    let outstanding = JOBS.lock().map(|held| !held.is_empty()).unwrap_or(false);
    if outstanding { entry::Pending::In(Duration::ZERO) } else { entry::Pending::Idle }
}

/// Queues a derived key for delivery on the next pump, or raises.
///
/// `undefined` in both cases: every one of Node's callback KDFs is `=> void`.
fn settle(callback: u64, derived: Result<Vec<u8>, String>) -> u64 {
    match derived {
        Ok(bytes) => {
            if let Ok(mut held) = JOBS.lock() {
                held.push(Job { callback, bytes });
            }
        }
        Err(message) => entry::throw_type_error(&message),
    }
    entry::undefined_value()
}

/// The refusal Node raises when the callback argument is missing.
///
/// Checked before the derivation runs, not after: deriving a key for nobody is
/// seconds of scrypt spent to reach an error the arguments already contained.
fn require_callback(callback: u64) -> Result<u64, String> {
    if callback == entry::undefined_value() || callback == entry::null_value() {
        return Err("The \"callback\" argument must be of type function.".to_owned());
    }
    Ok(callback)
}

/// Raises and answers `undefined` — the shape every entry point here ends in
/// when it cannot proceed.
fn refuse(message: &str) -> u64 {
    entry::throw_type_error(message);
    entry::undefined_value()
}

/// `crypto.pbkdf2(password, salt, iterations, keylen, digest, callback)`.
///
/// Six arguments, two past the convention's four. Both come through
/// `util::extra_argument`, which reads the activation's own argument vector —
/// index 4 is `digest` and index 5 is `callback`.
pub(super) extern "C" fn pbkdf2(
    _e: u64,
    _this: u64,
    password: u64,
    salt: u64,
    iterations: u64,
    keylen: u64,
) -> u64 {
    let callback = util::extra_argument(5, password, salt, iterations, keylen);
    let callback = match require_callback(callback) {
        Ok(callback) => callback,
        Err(message) => return refuse(&message),
    };
    let read = Pbkdf2Arguments::read(password, salt, iterations, keylen);
    settle(callback, sync::derive_pbkdf2(&read))
}

/// `crypto.hkdf(digest, ikm, salt, info, keylen, callback)`.
///
/// The key handed to the callback is a `Buffer` where Node hands an
/// `ArrayBuffer` — `super::sync::hkdf_sync`'s doc has the reason, which is the
/// same one, and it is not restated here.
pub(super) extern "C" fn hkdf(
    _e: u64,
    _this: u64,
    digest: u64,
    ikm: u64,
    salt: u64,
    info: u64,
) -> u64 {
    let callback = util::extra_argument(5, digest, ikm, salt, info);
    let callback = match require_callback(callback) {
        Ok(callback) => callback,
        Err(message) => return refuse(&message),
    };
    let read = HkdfArguments::read(digest, ikm, salt, info);
    settle(callback, sync::derive_hkdf(&read))
}

/// `crypto.scrypt(password, salt, keylen[, options], callback)`.
///
/// # The four-argument overload
///
/// `options` is optional, so the callback is the FOURTH argument when it is
/// omitted and the fifth when it is not. Which one it is, is decided by whether
/// a fifth argument exists at all — not by testing the fourth for callability,
/// which a host crate has no predicate for. A caller that passes an options
/// object and no callback therefore gets "callback must be a function" naming
/// the object, which is the same complaint Node makes about the same mistake.
pub(super) extern "C" fn scrypt(
    _e: u64,
    _this: u64,
    password: u64,
    salt: u64,
    keylen: u64,
    fourth: u64,
) -> u64 {
    let fifth = util::extra_argument(4, password, salt, keylen, fourth);
    let (options, callback) =
        if fifth == entry::undefined_value() { (entry::undefined_value(), fourth) } else { (fourth, fifth) };
    let callback = match require_callback(callback) {
        Ok(callback) => callback,
        Err(message) => return refuse(&message),
    };
    // Read explicitly rather than through `ScryptArguments::read`, because which
    // slot holds the options was decided above and that reader takes it as
    // given.
    let derived = {
        let len = entry::number_of(keylen).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0) as usize;
        let password = util::binary_like(password);
        let salt = util::binary_like(salt);
        let options = entry::with_runtime(|context| ScryptOptions::read(context, options));
        sync::derive_scrypt(&ScryptArguments { password, salt, keylen: len, options })
    };
    settle(callback, derived)
}
