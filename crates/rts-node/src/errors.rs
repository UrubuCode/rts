//! The `ERR_*` errors Node's own APIs raise when an argument is wrong.
//!
//! # Why a program does not read the message, it reads the CODE
//!
//! ```js
//! assert.throws(() => fs.chownSync(1, 1, 1), {
//!   code: 'ERR_INVALID_ARG_TYPE',
//!   name: 'TypeError',
//! });
//! ```
//!
//! That is the shape all through Node's own suite, and it means the error is an
//! interface with three fields — `name`, `code`, `message` — that every module
//! must produce identically. Five modules each writing their own is five chances
//! to spell `ERR_INVALID_ARG_TYPE` differently, and the one that differs fails a
//! test that says nothing about the module it is testing.
//!
//! Measured 2026-08-24 against Node's suite: **148 files** die on *"Missing
//! expected exception"* — an API of ours that accepted what Node refuses. They
//! are spread over `fs` (37), `buffer` (17), the WHATWG globals (17), `zlib`
//! (12), `child_process`, `net`, `process` and `worker_threads`.
//!
//! # Why the raising itself is no longer HERE
//!
//! It was, and it moved down to `rts_core::entry::errors` for one caller it
//! could not serve: **`Buffer` is not in this crate.** The class lives in
//! `rts-core` (`layering.md` §6: bytes and codecs need no operating system), so
//! its argument checks cannot call a `pub(crate)` function of a crate that
//! depends on `rts-core`. The rejected alternative was a second copy beside
//! `Buffer`, which is precisely the divergence the paragraph above exists to
//! refuse.
//!
//! What is left here is the spelling this crate's modules already call, over the
//! one implementation. `fs::validate` and its neighbours did not have to notice.

use rts_core::entry;

/// Raises `TypeError [ERR_INVALID_ARG_TYPE]`.
///
/// `name` is the argument as the documentation spells it (`"uid"`, `"path"`),
/// `expected` is what it must be (`"number"`, `"string or Buffer"`), and
/// `actual` is what arrived.
pub(crate) fn invalid_arg_type(name: &str, expected: &str, actual: u64) {
    entry::invalid_arg_type(name, expected, actual);
}

/// Raises `RangeError [ERR_OUT_OF_RANGE]`.
pub(crate) fn out_of_range(name: &str, expected: &str, actual: u64) {
    entry::out_of_range(name, expected, actual);
}

/// Raises `TypeError [ERR_INVALID_ARG_VALUE]`.
pub(crate) fn invalid_arg_value(name: &str, actual: u64, reason: &str) {
    entry::invalid_arg_value(name, actual, reason);
}

/// Raises `TypeError [ERR_INVALID_ARG_TYPE]` for a wrong CLASS — the *"must be
/// an instance of Array"* wording, against [`invalid_arg_type`]'s *"must be of
/// type number"*. `process.hrtime` asserts the sentence literally.
pub(crate) fn invalid_arg_instance(name: &str, expected: &str, actual: u64) {
    entry::invalid_arg_instance(name, expected, actual);
}

/// Raises `TypeError [ERR_MISSING_ARGS]`.
///
/// `names` are quoted and joined with `or`, which is Node's phrasing for the
/// case where any one of several would do — `net.connect()` wants
/// `"options" or "port" or "path"`.
pub(crate) fn missing_args(names: &[&str]) {
    let quoted: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    let joined = quoted.join(" or ");
    raise(
        "TypeError",
        "ERR_MISSING_ARGS",
        &format!("The {joined} argument must be specified"),
    );
}

/// Raises `RangeError [ERR_SOCKET_BAD_PORT]`.
///
/// A code of its own and not [`out_of_range`]: `node:net`'s suite asserts
/// `ERR_SOCKET_BAD_PORT` for a port outside `0..=65535` while asserting
/// `ERR_INVALID_ARG_TYPE` for a port that is neither a number nor a string, so
/// the two refusals cannot share one raiser.
///
/// `name` is spelled as the message spells it — `"port"` positionally,
/// `"options.port"` as an option — because that text is what Node prints.
///
/// # Why [`socket_bad_port`] is beside this and not the same function
///
/// The code is one code and the RANGE is not: `net` accepts port `0` (ask the
/// OS for one), `dgram` does not, and Node's two messages differ by that one
/// character — *">= 0 and < 65536"* here, *">= 1 and < 65536"* there — with
/// each module's suite matching its own text. Collapsing them would mean a
/// floor parameter at every call site, which is the same fork with more places
/// to get it wrong. Named here so the pair reads as a decision.
pub(crate) fn bad_port(name: &str, actual: u64) {
    let described = described(actual);
    raise(
        "RangeError",
        "ERR_SOCKET_BAD_PORT",
        &format!("{name} should be >= 0 and < 65536. Received {described}."),
    );
}

/// Raises `TypeError [ERR_INVALID_ARG_VALUE]` about a PROPERTY of an options
/// bag rather than about an argument.
///
/// Node's own builder switches to *"The property '…'"* whenever the name it is
/// given carries a dot; [`invalid_arg_value`] always writes *"The argument
/// '…'"*, and `net`'s `objectMode` test compares the property spelling. `name`
/// is written whole — `"options.objectMode"` — because only the caller knows
/// which bag the property came out of.
pub(crate) fn unsupported_property(name: &str, actual: u64) {
    let described = described(actual);
    raise(
        "TypeError",
        "ERR_INVALID_ARG_VALUE",
        &format!("The property '{name}' is not supported. Received {described}"),
    );
}

/// Raises `TypeError [ERR_UNKNOWN_SIGNAL]`.
///
/// The name arrives as Rust text, not as a value: the caller has already had to
/// read it in order to fail to find it in its signal table.
pub(crate) fn unknown_signal(name: &str) {
    raise("TypeError", "ERR_UNKNOWN_SIGNAL", &format!("Unknown signal: {name}"));
}

/// Raises a plain `Error` carrying an errno `code` — `EINVAL`, `ESRCH`,
/// `EPERM` — in the `"{syscall} {code}"` shape libuv gives them.
///
/// Not a `TypeError`: this is the OS refusing a well-formed request, and Node's
/// suite asserts `name: 'Error'` for exactly that reason (`process.kill(0, 987)`
/// is the case in the corpus this was written for).
pub(crate) fn system_error(syscall: &str, code: &str) {
    raise("Error", code, &format!("{syscall} {code}"));
}

/// Builds an error of `class`, stamps `code` on it, and raises it.
///
/// # Why this exists beside `rts_core::entry::errors`'s own `raise`
///
/// That one is private, and the codes above are `node:`-only — `ERR_MISSING_ARGS`
/// and `ERR_SOCKET_BAD_PORT` mean nothing to a `Buffer`, which is the caller
/// the raising moved down to `rts-core` to serve (see this module's header).
/// So the split is by AUDIENCE and not by accident: what every surface shares
/// lives there and is re-spelled above, and what only `node:` raises is raised
/// here. The alternative — making that `raise` public and calling it — is a
/// change in `rts-core` and belongs to whoever owns both sides at once.
///
/// The class name crosses as text for the reason `rts-core`'s own copy states:
/// a `TypeError` raised here must be the same `TypeError` the program's
/// `catch (e) { e instanceof TypeError }` compares against, and
/// [`entry::make_named_error`] is what reaches the one the runtime installed.
fn raise(class: &str, code: &str, message: &str) {
    let Some(error) = entry::make_named_error(class, message) else {
        // No class to build from: a program so early that the primordials are
        // not installed. Raising nothing would let the call carry on with the
        // argument it was going to refuse, so the plain type error stands in.
        entry::throw_type_error(message);
        return;
    };
    let code = code.to_owned();
    entry::with_runtime(|context| {
        let held = entry::make_string(context, &code);
        entry::put_member(context, error, "code", held);
    });
    entry::throw_value(error);
}

/// A value as a message renders it — `"Received 65536."`, `"Received true"`.
fn described(value: u64) -> String {
    entry::with_runtime(|context| entry::text_in(context, value))
        .unwrap_or_else(|| String::from("undefined"))
}

/// Raises `RangeError [ERR_BUFFER_OUT_OF_BOUNDS]`, naming the bound that left
/// the buffer — `"offset"` or `"length"`.
///
/// The code is the SAME for both, so the name is the only thing that tells a
/// caller which one it was; `dgram`'s own suite asserts each message
/// literally. `Option` rather than `&str` because the runtime's form also
/// answers the nameless "ran off the end by itself" case, which this crate
/// does not raise but must not fork over.
pub(crate) fn buffer_out_of_bounds(name: &str) {
    entry::buffer_out_of_bounds(Some(name));
}

/// Raises `TypeError [ERR_SOCKET_BAD_TYPE]`.
///
/// No argument: the code names exactly one mistake — a `dgram.createSocket`
/// whose type is neither `'udp4'` nor `'udp6'` — and Node's message names the
/// two valid answers rather than the wrong one that arrived, so there is
/// nothing to interpolate. `test-dgram-createSocket-type.js` matches that text
/// with a `RegExp` anchored at both ends, which is why it is spelled once here
/// instead of being assembled at each call.
pub(crate) fn socket_bad_type() {
    raise(
        "TypeError",
        "ERR_SOCKET_BAD_TYPE",
        "Bad socket type specified. Valid types are: udp4, udp6",
    );
}

/// Raises `RangeError [ERR_SOCKET_BAD_PORT]`.
///
/// A **RangeError**, which is the whole reason this is not [`out_of_range`]
/// with a port-shaped range string: `test-dgram-send-bad-arguments.js` asserts
/// the CLASS (`assert.throws(…, RangeError)`) for a port of `-1`, `0` and
/// `65536`, and Node gives that refusal its own code. `name` is capitalised as
/// Node capitalises it (`"Port"`) because the message opens with it.
pub(crate) fn socket_bad_port(name: &str, actual: u64) {
    let described = described(actual);
    raise(
        "RangeError",
        "ERR_SOCKET_BAD_PORT",
        &format!("{name} should be >= 1 and < 65536. Received {described}."),
    );
}

/// Raises `Error [ERR_SOCKET_DGRAM_IS_CONNECTED]`.
///
/// A plain `Error`, not a `TypeError` — this is not an argument of the wrong
/// shape but a right one offered at the wrong time: a destination handed to a
/// socket whose `connect()` already fixed one. Node's suite asserts
/// `name: 'Error'`, so raising a `TypeError` would fail a test that never
/// looks at the message.
pub(crate) fn socket_dgram_is_connected() {
    raise("Error", "ERR_SOCKET_DGRAM_IS_CONNECTED", "Already connected");
}
