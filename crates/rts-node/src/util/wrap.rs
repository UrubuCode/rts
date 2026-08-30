//! `deprecate`, `debuglog`/`debug`, `callbackify` — the members that return a
//! function wrapping the one they were given.
//!
//! # What reuse-check found, and what it overturned
//!
//! This module's own refusal list said all three were impossible: a wrapper has
//! to remember the function it wraps, and `entry::make_callable` hands back a
//! bare `extern "C" fn` with no environment slot. That is true of
//! `make_callable` and **false of the surface** —
//! [`rts_core::entry::closure_new`] takes a code address AND an
//! environment, and the environment arrives as the callee's FIRST argument,
//! which is the `_e` slot every other native in this crate ignores.
//!
//! `crates/rts-node/src/perf_hooks/timerify.rs` had already found this and
//! written it down. Nothing new is invented here: the environment is an array
//! read back with the ambient `get_indexed`, exactly as that module does, so
//! there is one way a native closure carries state rather than two.
//!
//! # Why `promisify` is still refused
//!
//! It needs a promise that is PENDING when it is answered and settles when a
//! callback fires later. `entry::settled` makes an already-settled one and
//! nothing on the host surface makes a pending one with a resolver — see the
//! module doc.

use std::collections::HashSet;
use std::sync::Mutex;

use rts_core::entry;

use super::values::{get, is_callable, present, string_of};

/// Which deprecation codes have already been reported.
///
/// Process-wide and keyed by CODE, which is Node's own rule: two different
/// functions deprecated under one code warn once between them, and a
/// deprecation with no code warns on every call because there is no key to
/// collapse against.
static REPORTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The environment slots, by index. Named because a native reads them back by
/// number and a bare `2` in the middle of a call says nothing.
const TARGET: f64 = 0.0;
const FIRST: f64 = 1.0;
const SECOND: f64 = 2.0;

/// `util.deprecate(fn, msg[, code[, options]])`.
pub(super) extern "C" fn deprecate(
    _e: u64,
    _this: u64,
    target: u64,
    message: u64,
    code: u64,
    options: u64,
) -> u64 {
    if !is_callable(target) {
        return entry::undefined_value();
    }
    let environment = entry::make_array(vec![target, message, code]);
    let wrapper = entry::closure_new(deprecated as *const () as usize as i64, environment);
    // `modifyPrototype` defaults to true, and what it copies is the prototype —
    // which is what makes `util.deprecate(SomeClass, …)` still usable with
    // `new`.
    let modify = match super::values::is_object_value(options) {
        true => entry::to_boolean(super::values::option(options, "modifyPrototype")),
        false => true,
    };
    if modify {
        let prototype = get(target, "prototype");
        if present(prototype) {
            entry::set_indexed(wrapper, super::values::string("prototype"), prototype, 0 /* strict: quem escreve a partir do host reporta a recusa */);
        }
    }
    wrapper
}

/// The wrapper a program actually calls.
extern "C" fn deprecated(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let target = entry::get_indexed(environment, entry::make_number(TARGET));
    let message = string_of(entry::get_indexed(environment, entry::make_number(FIRST)))
        .unwrap_or_else(|| "deprecated".to_owned());
    let code = string_of(entry::get_indexed(environment, entry::make_number(SECOND)));
    if should_report(code.as_deref()) {
        match &code {
            Some(code) => eprintln!("(rts:{}) [{code}] DeprecationWarning: {message}", std::process::id()),
            None => eprintln!("(rts:{}) DeprecationWarning: {message}", std::process::id()),
        }
    }
    // Read ambiently, then call ambiently: nothing here holds a borrow across
    // `entry::call`, which runs user code.
    entry::call(target, this, a0, a1, a2, a3)
}

/// Whether this warning has not been reported yet. Always true with no code.
fn should_report(code: Option<&str>) -> bool {
    let Some(code) = code else {
        return true;
    };
    let Ok(mut held) = REPORTED.lock() else {
        // A poisoned lock means another thread panicked mid-report. Warning
        // twice is better than the alternative here, which is warning never.
        return true;
    };
    held.get_or_insert_with(HashSet::new).insert(code.to_owned())
}

/// `util.debuglog(section[, callback])`, and its alias `util.debug`.
///
/// The returned function carries `.enabled`, which is the whole point of the
/// API: a caller checks it before building an expensive argument.
pub(super) extern "C" fn debuglog(
    _e: u64,
    _this: u64,
    section: u64,
    callback: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let named = string_of(section).unwrap_or_default();
    let enabled = section_enabled(&named);
    let environment = entry::make_array(vec![section]);
    let code = match enabled {
        true => write_debug as *const () as usize as i64,
        false => ignore_debug as *const () as usize as i64,
    };
    let logger = entry::closure_new(code, environment);
    entry::with_runtime(|context| {
        entry::put_member(context, logger, "enabled", entry::boolean_value(enabled));
    });
    // Node calls the optimizer callback lazily, the first time the section is
    // actually enabled. There is nothing here to hook a "first use" onto, so it
    // is called now when the section is enabled and never when it is not —
    // which preserves the property callers rely on (an expensive setup does not
    // run for a disabled section) and diverges only in WHEN it runs.
    if enabled && is_callable(callback) {
        let absent = entry::undefined_value();
        entry::call(callback, absent, logger, absent, absent, absent);
    }
    logger
}

/// Whether `NODE_DEBUG` selects a section, with Node's `*` wildcard.
///
/// Read fresh on every `debuglog` call rather than cached: a cache would be a
/// second answer to what the environment says, and this is called a handful of
/// times per program.
fn section_enabled(section: &str) -> bool {
    let Ok(asked) = std::env::var("NODE_DEBUG") else {
        return false;
    };
    asked
        .split(',')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| match pattern.strip_suffix('*') {
            Some(prefix) => section.to_lowercase().starts_with(&prefix.to_lowercase()),
            None => pattern.eq_ignore_ascii_case(section),
        })
}

/// The enabled logger: `util.format` to stderr, prefixed the way Node's is.
extern "C" fn write_debug(environment: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let section = string_of(entry::get_indexed(environment, entry::make_number(TARGET)))
        .unwrap_or_default();
    let text = super::inspect::formatted(super::inspect::Options::default(), a0, [a1, a2, a3]);
    eprintln!("{} {}: {text}", section.to_uppercase(), std::process::id());
    entry::undefined_value()
}

/// The disabled logger. It must still be a function with `.enabled`, which is
/// why this exists rather than the namespace answering `undefined`.
extern "C" fn ignore_debug(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// `util.callbackify(original)`.
///
/// # What crosses, and what does not
///
/// Three arguments and the callback, because the convention has four slots. A
/// falsy rejection reason reaches the callback AS IT IS, where Node wraps it in
/// a fresh `Error` carrying `.reason` — nothing here can construct an `Error`
/// the way the language does, and a plain object shaped like one would be a
/// fabricated value rather than a gap.
pub(super) extern "C" fn callbackify(
    _e: u64,
    _this: u64,
    original: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    if !is_callable(original) {
        return entry::undefined_value();
    }
    entry::closure_new(callbacked as *const () as usize as i64, entry::make_array(vec![original]))
}

/// The callback-style function `callbackify` answers.
extern "C" fn callbacked(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let target = entry::get_indexed(environment, entry::make_number(TARGET));
    let absent = entry::undefined_value();
    // The callback is the LAST argument, which with fixed slots means the last
    // callable one that was passed. Guessing by position instead would call an
    // omitted trailing argument.
    let given = [a0, a1, a2, a3];
    let Some(at) = given.iter().rposition(|value| present(*value) && is_callable(*value)) else {
        return absent;
    };
    let mut forwarded = [absent; 4];
    forwarded[..at].copy_from_slice(&given[..at]);
    let callback = given[at];

    let produced = entry::call(
        target,
        this,
        forwarded[0],
        forwarded[1],
        forwarded[2],
        forwarded[3],
    );
    let then = get(produced, "then");
    if !is_callable(then) {
        return absent;
    }
    let settled = entry::make_array(vec![callback]);
    let on_value = entry::closure_new(fulfilled as *const () as usize as i64, settled);
    let on_reason = entry::closure_new(rejected as *const () as usize as i64, settled);
    entry::call(then, produced, on_value, on_reason, absent, absent);
    absent
}

/// `then`'s fulfilment reaction: `callback(null, value)`.
extern "C" fn fulfilled(environment: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let callback = entry::get_indexed(environment, entry::make_number(TARGET));
    let absent = entry::undefined_value();
    entry::call(callback, absent, entry::null_value(), value, absent, absent)
}

/// `then`'s rejection reaction: `callback(reason)`.
extern "C" fn rejected(environment: u64, _this: u64, reason: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let callback = entry::get_indexed(environment, entry::make_number(TARGET));
    let absent = entry::undefined_value();
    entry::call(callback, absent, reason, absent, absent, absent)
}
