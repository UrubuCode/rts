//! What each `node:fs` member ACCEPTS, checked before it does any work.
//!
//! # Why a module of its own, and not a check per member
//!
//! Because the rule is the same in thirty places and the message is not
//! negotiable. `fs.rename`, `fs.unlink`, `fs.rmdir`, `fs.readlink` and every
//! other path-taking member refuse exactly the same set of values with exactly
//! the same `ERR_INVALID_ARG_TYPE`, and Node's own suite asserts the text:
//!
//! ```js
//! message: 'The "oldPath" argument must be of type string or an instance ' +
//!          'of Buffer or URL. Received type number (1)'
//! ```
//!
//! Thirty hand-written checks are thirty chances to phrase that differently,
//! and the one that differs fails a test that says nothing about the member it
//! is testing. So the DECIDING lives here, once per rule, and the RAISING stays
//! in `crate::errors` — which is the module doc there saying the same thing
//! from the other side: this is the `internal/validators` half it deliberately
//! does not carry, kept in the surface whose rules it encodes.
//!
//! # Every function here answers `Option`, and `None` means "already raised"
//!
//! A native cannot unwind, so a refusal is a throw REGISTERED plus a return.
//! The shape a caller writes is therefore always the same:
//!
//! ```ignore
//! let Some(path) = validate::path("path", path) else {
//!     return entry::undefined_value();
//! };
//! ```
//!
//! Returning immediately is not a style preference: [`crate::errors`] has left
//! a pending throw behind, and carrying on would do the very work the refusal
//! exists to prevent, with the throw landing afterwards over a side effect that
//! already happened.
//!
//! # Never called from inside `with_runtime`
//!
//! Every function here opens its own borrow to READ the value, closes it, and
//! only then raises — because raising opens one of its own
//! (`crate::errors` builds a string for the message). A nested borrow in an
//! `extern "C"` frame cannot unwind and aborts the process, which is why the
//! reads below are collected into a plain Rust value first rather than checked
//! in place.

use rts_core::entry::{self, Context};

/// A path argument as text — Node's `string | Buffer | URL`.
///
/// `name` is the argument as Node's own message spells it (`"path"`,
/// `"oldPath"`, `"src"`), because that text is what a test compares.
///
/// Two refusals live here rather than in the callers:
/// - a value that is none of the three types — `ERR_INVALID_ARG_TYPE`;
/// - a path carrying a NUL byte — `ERR_INVALID_ARG_VALUE`, which is a separate
///   code in Node precisely because the type was right and the content was not
///   (`test-fs-null-bytes.js` asserts it for every path-taking member).
pub(super) fn path(name: &str, value: u64) -> Option<String> {
    let read = entry::with_runtime(|context| read_path(context, value));
    let Some(text) = read else {
        crate::errors::invalid_arg_type(name, "string or an instance of Buffer or URL", value);
        return None;
    };
    if text.contains('\0') {
        crate::errors::invalid_arg_value(
            name,
            value,
            "must be a string or Uint8Array without null bytes",
        );
        return None;
    }
    // The delivery point every other argument reader in this module is: see
    // `super::text`'s doc for why a queued `watch` event is pumped from here
    // rather than from an event loop this engine does not have. Validation
    // taking over the first read of an argument would otherwise have silently
    // removed the only place those events are ever delivered.
    super::watch::pump();
    Some(text)
}

/// The three accepted spellings of a path, read under one borrow.
///
/// `None` for everything else, INCLUDING a `URL` whose scheme is not `file:` —
/// Node answers `ERR_INVALID_URL_SCHEME` there and this answers
/// `ERR_INVALID_ARG_TYPE`. A stated divergence rather than a silent one: the
/// alternative is a fifth error constructor in `crate::errors` used by exactly
/// one caller, and no test in the corpus this serves asserts that code.
fn read_path(context: &mut Context, value: u64) -> Option<String> {
    if let Some(text) = entry::string_in(context, value) {
        return Some(text);
    }
    if let Some(bytes) = entry::bytes_of(context, value) {
        return Some(String::from_utf8_lossy(&bytes).into_owned());
    }
    if !entry::is_object(context, value) || entry::is_callable_in(context, value) {
        return None;
    }
    let protocol = entry::get_member(context, value, "protocol");
    if entry::string_in(context, protocol).as_deref() != Some("file:") {
        return None;
    }
    let pathname = entry::get_member(context, value, "pathname");
    let pathname = entry::string_in(context, pathname)?;
    Some(file_url_path(&pathname))
}

/// A `file:` URL's `pathname` as a filesystem path.
///
/// Only the two transformations a path actually needs: percent-decoding, and
/// dropping the leading `/` that precedes a Windows drive letter (`/C:/x` is
/// `C:/x`). The host half of the URL — a UNC share in Node — is not read,
/// because [`read_path`] never looks at it; a `file://server/share` path
/// resolves to its `pathname` alone here, which is the one URL shape this does
/// not answer the way Node does.
fn file_url_path(pathname: &str) -> String {
    let decoded = percent_decoded(pathname);
    let bytes = decoded.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes[2] == b':' || bytes[2] == b'|');
    match windows_drive {
        true => decoded[1..].replace('|', ":"),
        false => decoded,
    }
}

/// `%XX` decoded, everything else copied. Invalid escapes are left as written
/// rather than dropped — a path is a name, and a name nobody can spell back is
/// worse than one that fails to open.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        // Read as BYTES, never as a `&str` slice: `%` may be followed by a
        // multi-byte character, and slicing `text[index + 1..index + 3]` there
        // is a panic on a non-boundary — which in an `extern "C"` frame cannot
        // unwind and takes the process with it.
        let decoded = match bytes[index] == b'%' && index + 2 < bytes.len() {
            true => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                high.zip(low).map(|(high, low)| (high * 16 + low) as u8)
            }
            false => None,
        };
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file descriptor — Node's `validateInt32(fd, 'fd', 0, 2147483647)`.
///
/// The three refusals are three different codes, and the tests assert which:
/// a non-number is `ERR_INVALID_ARG_TYPE`, a fractional or non-finite number is
/// `ERR_OUT_OF_RANGE` reading *"must be an integer"*, and a negative or
/// oversized one is `ERR_OUT_OF_RANGE` reading *">= 0 && <= 2147483647"*.
pub(super) fn fd(value: u64) -> Option<i64> {
    integer("fd", value, 0.0, 2_147_483_647.0).map(|number| number as i64)
}

/// A uid or gid — Node's `validateInteger(value, name, -1, 4294967295)`.
/// `-1` is the "leave unchanged" sentinel `chown(2)` itself uses, which is why
/// the floor is not zero.
pub(super) fn id(name: &str, value: u64) -> Option<i64> {
    integer(name, value, -1.0, 4_294_967_295.0).map(|number| number as i64)
}

/// A file mode — Node's `parseFileMode`, which takes a number OR an octal
/// string (`fs.chmodSync(p, '0644')` is how a program writes it).
///
/// A string that is not octal is `ERR_INVALID_ARG_VALUE` and not
/// `ERR_INVALID_ARG_TYPE`: `'123x'` IS a string, which is an accepted type, so
/// what failed is the value. `test-fs-fchmod.js` asserts exactly that pair.
pub(super) fn mode(name: &str, value: u64) -> Option<u32> {
    let text = entry::with_runtime(|context| entry::string_in(context, value));
    if let Some(text) = text {
        return match u32::from_str_radix(text.trim(), 8) {
            Ok(mode) => Some(mode),
            Err(_) => {
                crate::errors::invalid_arg_value(
                    name,
                    value,
                    "must be a 32-bit unsigned integer or an octal string",
                );
                None
            }
        };
    }
    integer(name, value, 0.0, 4_294_967_295.0).map(|number| number as u32)
}

/// An integer argument inside `[min, max]`, raising the code that matches WHICH
/// of the three things went wrong. The shared body of [`fd`], [`id`], [`mode`]
/// and the byte-offset checks — the rule written once, per this crate's README.
pub(super) fn integer(name: &str, value: u64, min: f64, max: f64) -> Option<f64> {
    let Some(number) = entry::number_of(value) else {
        crate::errors::invalid_arg_type(name, "number", value);
        return None;
    };
    if !number.is_finite() || number.fract() != 0.0 {
        crate::errors::out_of_range(name, "an integer", value);
        return None;
    }
    if number < min || number > max {
        crate::errors::out_of_range(name, &format!(">= {min} && <= {max}"), value);
        return None;
    }
    Some(number)
}

/// An OPTIONAL byte length — `truncateSync(path)` and
/// `ftruncateSync(fd)` both mean zero when it is absent, which is why this
/// answers `Some(0)` for `undefined` rather than refusing.
///
/// A present value is an ordinary non-negative integer, so a `-1` or a `1.5`
/// is refused here rather than silently clamped by an `as u64` cast — which is
/// what the `.max(0.0) as u64` this replaced did: `truncateSync(p, -1)` emptied
/// the file instead of complaining.
pub(super) fn length(name: &str, value: u64) -> Option<u64> {
    if value == entry::undefined_value() {
        return Some(0);
    }
    integer(name, value, 0.0, 9_007_199_254_740_991.0).map(|number| number as u64)
}

/// The mandatory callback of an err-first async form.
///
/// The argument is named `"cb"` in the message because that is what Node's
/// `makeCallback`/`makeStatsCallback` call it — `test-fs-make-callback.js` and
/// `test-fs-makeStatsCallback.js` are the two files that exist to pin it.
///
/// `false` means the throw is already registered and the caller must return.
pub(super) fn callback(value: u64) -> bool {
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, value));
    if !callable {
        crate::errors::invalid_arg_type("cb", "function", value);
    }
    callable
}

/// An optional integer read off an OPTIONS object — `{ start, end }`.
///
/// `true` when it is absent or acceptable, `false` when the refusal has been
/// raised. It is a separate function from [`integer`] because "absent" is a
/// legal answer here and is not one for a positional argument: an options key
/// nobody wrote must not be refused for not being a number.
pub(super) fn option_integer(options: u64, name: &str) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return true;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    if value == absent {
        return true;
    }
    integer(name, value, 0.0, 9_007_199_254_740_991.0).is_some()
}

/// A buffer argument — Node's `validateBuffer`, whose message names the three
/// shapes it accepts rather than the one type it checks for.
///
/// Answers the window's LENGTH rather than its bytes: every caller needs to
/// know how big the window is in order to range-check `offset`/`length`
/// against it, and none of them wants the copy [`entry::bytes_of`] makes.
pub(super) fn buffer(name: &str, value: u64) -> Option<usize> {
    let window = entry::with_runtime(|context| entry::bytes_of(context, value).map(|held| held.len()));
    if window.is_none() {
        crate::errors::invalid_arg_type(
            name,
            "an instance of Buffer, TypedArray, or DataView",
            value,
        );
    }
    window
}
