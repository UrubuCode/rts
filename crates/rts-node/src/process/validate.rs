//! What each `node:process` member ACCEPTS, checked before it does any work.
//!
//! # Why a module of its own
//!
//! Two reasons, and the first is the one `fs::validate` already states: the
//! rules are shared (`exit(code)` and `_kill(pid)` are checked identically on
//! both platforms) and the CODE a refusal carries is what Node's suite asserts,
//! so deciding it in four places is four chances to pick the wrong one.
//!
//! The second is specific to this module: `lifecycle.rs` carries two `#[cfg]`
//! forms of `kill` and of `umask`, and hanging the argument checks off each of
//! them would have written every rule twice — once for POSIX and once for
//! Windows — with only one of the two ever compiled at a time, which is the
//! shape where a divergence survives review. The signal TABLE is genuinely
//! per-platform (`SIGUSR1` is 10 on Linux and 30 on macOS) and lives here
//! beside the check that reads it; everything above it is written once.
//!
//! # Every function answers `Option`, and `None` means "already raised"
//!
//! A native cannot unwind, so a refusal is a throw REGISTERED plus a return:
//!
//! ```ignore
//! let Some(status) = validate::exit_code(code) else {
//!     return entry::undefined_value();
//! };
//! ```
//!
//! Returning immediately is not a style preference: [`crate::errors`] has left
//! a pending throw behind, and carrying on would end the process — or signal
//! one — with the value the refusal exists to reject.
//!
//! # Never called from inside `with_runtime`
//!
//! Each function opens its own borrow to READ, closes it, and only then raises;
//! raising opens one of its own, and a nested borrow in an `extern "C"` frame
//! cannot unwind and aborts the process.

use rts_core::entry;

/// A pid — Node's own `pid != (pid | 0)` test, which is a loose comparison and
/// therefore accepts the STRING form: `process.kill('0')` and
/// `process.kill(String(process.pid))` are both in its suite, beside `null`,
/// `NaN` and `Infinity`, which are all refused.
///
/// One rule rather than "a number, or a string that parses": the truncation
/// test is what makes `'0'` pass and `2.5` fail, and writing it as two rules
/// would need a third to keep them agreeing.
pub(super) fn pid(value: u64) -> Option<i32> {
    let number = match entry::number_of(value) {
        Some(number) => Some(number),
        None => entry::with_runtime(|context| entry::string_in(context, value))
            .and_then(|text| text.trim().parse::<f64>().ok()),
    };
    let whole = number.filter(|number| {
        number.is_finite()
            && number.fract() == 0.0
            && *number >= f64::from(i32::MIN)
            && *number <= f64::from(i32::MAX)
    });
    match whole {
        Some(number) => Some(number as i32),
        None => {
            crate::errors::invalid_arg_type("pid", "number", value);
            None
        }
    }
}

/// `kill`'s signal argument as a number: absent is `SIGTERM`, a number is
/// itself, a name is looked up in [`named_signal`].
///
/// The two refusals are two different classes, which Node's suite asserts
/// separately: an unrecognised NAME is `TypeError [ERR_UNKNOWN_SIGNAL]`,
/// because the vocabulary is fixed and the caller misspelled it, while a number
/// outside the signal range is `Error [EINVAL]` — the OS refusing a
/// well-formed request, which is not a type error at all.
///
/// An unrecognised name is refused rather than defaulted: delivering `SIGTERM`
/// for a name nobody recognised would terminate a process the program only
/// meant to poke.
pub(super) fn signal(value: u64) -> Option<i32> {
    if value == entry::undefined_value() {
        return Some(TERM);
    }
    if let Some(number) = entry::number_of(value) {
        // `0` is the standard existence-and-permission probe, so the range
        // starts there. The ceiling is generous on purpose — the real table
        // stops around 31, and a platform may add a few — but it is finite,
        // because `kill(pid, 987)` is the case Node answers `EINVAL` for.
        let known = number.is_finite() && number.fract() == 0.0 && (0.0..=64.0).contains(&number);
        if !known {
            crate::errors::system_error("kill", "EINVAL");
            return None;
        }
        return Some(number as i32);
    }
    let name = entry::with_runtime(|context| entry::string_in(context, value));
    let Some(name) = name else {
        crate::errors::invalid_arg_type("signal", "string or number", value);
        return None;
    };
    match named_signal(&name) {
        Some(number) => Some(number),
        None => {
            crate::errors::unknown_signal(&name);
            None
        }
    }
}

/// `SIGTERM`, the default this platform delivers.
#[cfg(unix)]
const TERM: i32 = libc::SIGTERM;

/// A signal by name, on POSIX.
///
/// Written against `libc`'s constants rather than the numbers, because
/// `SIGUSR1` is 10 on Linux and 30 on macOS — a literal table would be right on
/// one of them.
#[cfg(unix)]
fn named_signal(name: &str) -> Option<i32> {
    match name {
        "SIGHUP" => Some(libc::SIGHUP),
        "SIGINT" => Some(libc::SIGINT),
        "SIGQUIT" => Some(libc::SIGQUIT),
        "SIGABRT" => Some(libc::SIGABRT),
        "SIGKILL" => Some(libc::SIGKILL),
        "SIGALRM" => Some(libc::SIGALRM),
        "SIGTERM" => Some(libc::SIGTERM),
        "SIGUSR1" => Some(libc::SIGUSR1),
        "SIGUSR2" => Some(libc::SIGUSR2),
        "SIGPIPE" => Some(libc::SIGPIPE),
        "SIGCONT" => Some(libc::SIGCONT),
        "SIGSTOP" => Some(libc::SIGSTOP),
        _ => None,
    }
}

/// `SIGTERM` on Windows, where the number is libuv's and not the OS's.
#[cfg(not(unix))]
const TERM: i32 = 15;

/// A signal by name, on Windows.
///
/// The numbers are the ones libuv assigns and Node reports on this platform —
/// `SIGHUP` is 1 and `SIGTERM` is 15 there exactly as on POSIX, which is what
/// `test-process-kill-pid.js` pins. They are literals because Windows has no
/// header to take them from: nothing is DELIVERED here (see `lifecycle::kill`'s
/// Windows form, which terminates instead), so the number's only job is to be
/// the one a program reads back.
#[cfg(not(unix))]
fn named_signal(name: &str) -> Option<i32> {
    match name {
        "SIGHUP" => Some(1),
        "SIGINT" => Some(2),
        "SIGQUIT" => Some(3),
        "SIGILL" => Some(4),
        "SIGABRT" => Some(22),
        "SIGFPE" => Some(8),
        "SIGKILL" => Some(9),
        "SIGSEGV" => Some(11),
        "SIGTERM" => Some(15),
        "SIGBREAK" => Some(21),
        "SIGWINCH" => Some(28),
        _ => None,
    }
}

/// An exit status — Node's `validateExitCode`.
///
/// `undefined` and `null` are both `0`, which is Node's rule and not a
/// convenience: a program that ends without saying anything succeeded.
///
/// The two refusals are again two codes, and `test-process-exit-code-validation.js`
/// asserts which for each of eleven values: a number that is not a whole finite
/// one — `2.1`, `NaN`, `Infinity` — is `ERR_OUT_OF_RANGE`, because the type was
/// right; everything else, a non-integer STRING included, is
/// `ERR_INVALID_ARG_TYPE`. `'2'` is accepted, which is why the string is parsed
/// rather than refused outright.
pub(super) fn exit_code(value: u64) -> Option<i32> {
    let empty = entry::with_runtime(|context| {
        value == entry::undefined_in(context) || value == entry::null_in(context)
    });
    if empty {
        return Some(0);
    }
    if let Some(number) = entry::number_of(value) {
        if !number.is_finite() || number.fract() != 0.0 {
            crate::errors::out_of_range("code", "an integer", value);
            return None;
        }
        return Some(number as i32);
    }
    let parsed = entry::with_runtime(|context| entry::string_in(context, value))
        .and_then(|text| text.trim().parse::<i32>().ok());
    match parsed {
        Some(code) => Some(code),
        None => {
            crate::errors::invalid_arg_type("code", "number", value);
            None
        }
    }
}

/// `chdir`'s argument.
///
/// `string_in` asks whether the argument IS a string; `text_of` would hand back
/// `"undefined"` for a missing one and try to enter a directory of that name,
/// which is the coercion-as-a-type-test defect this repository has paid for
/// three times.
pub(super) fn directory(value: u64) -> Option<String> {
    match entry::with_runtime(|context| entry::string_in(context, value)) {
        Some(text) => Some(text),
        None => {
            crate::errors::invalid_arg_type("directory", "string", value);
            None
        }
    }
}

/// `hrtime`'s optional `[seconds, nanoseconds]` argument.
///
/// `false` means the throw is already registered and the caller must return.
///
/// Absent is fine — that is the absolute-reading form. Anything else must be an
/// Array of exactly two elements, and the LENGTH failure is `ERR_OUT_OF_RANGE`
/// rather than a type error: `[1]` is an array, so what is wrong with it is how
/// many elements it has, and Node's message says so — *"The value of "time" is
/// out of range. It must be 2. Received 1"*.
pub(super) fn time_tuple(value: u64) -> bool {
    if value == entry::undefined_value() {
        return true;
    }
    let length = entry::with_runtime(|context| {
        if !entry::is_array_in(context, value) {
            return None;
        }
        let held = entry::get_member(context, value, "length");
        Some(entry::number_of(held).unwrap_or(0.0))
    });
    let Some(length) = length else {
        crate::errors::invalid_arg_instance("time", "Array", value);
        return false;
    };
    if length != 2.0 {
        crate::errors::out_of_range("time", "2", entry::make_number(length));
        return false;
    }
    true
}
