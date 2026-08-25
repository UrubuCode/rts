//! Ending the program, moving it, and deferring work inside it.

use rts_core::entry;
use std::cell::RefCell;
use std::io::Write;
use std::time::Duration;

/// `process.cwd()`.
pub(super) extern "C" fn cwd(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// `process.chdir(directory)`.
///
/// A non-string argument is Node's `ERR_INVALID_ARG_TYPE` and is now RAISED —
/// the doc that stood here said a native cannot throw, which stopped being true
/// when `crate::errors` arrived; see `super::validate::directory`.
///
/// What is still not raised is the FAILURE: Node throws `ENOENT`/`ENOTDIR` when
/// the directory is well-named but not usable, and this reports that on stderr
/// and answers `undefined`, exactly as it does on success. A caller cannot
/// distinguish the two; `process.cwd()` afterwards can. Left as it is rather
/// than fixed in passing, because the errno an OS refusal carries has no reader
/// here yet and inventing one per call site is the duplication `crate::errors`
/// exists to prevent.
pub(super) extern "C" fn chdir(
    _e: u64,
    _this: u64,
    directory: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let absent = entry::undefined_value();
    let Some(path) = super::validate::directory(directory) else {
        return absent;
    };
    if let Err(error) = std::env::set_current_dir(&path) {
        eprintln!("rts: process.chdir({path}): {error}");
    }
    absent
}

/// `process.exit(code)` — ends the OS process.
///
/// `std::process::exit` does not return, does not run Rust destructors, and
/// does not let a calling harness observe the rest of its own test: a host
/// running a program that calls this takes the WHOLE process down. That is what
/// Node does — `process.exit()` ends the OS process — and reproducing anything
/// softer here would be the divergence, not the faithful behaviour.
///
/// `'exit'` is emitted first, synchronously, which is the one lifecycle event
/// this module can honestly fire: the listeners run while the runtime is still
/// intact, and nothing they schedule afterwards will ever run — matching Node's
/// own documented "synchronous work only" rule for that event.
pub(super) extern "C" fn exit(_e: u64, _this: u64, code: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(status) = status_for(code) else {
        // A refused code raises, and a raise must not be followed by the exit
        // it was raised to prevent: the program's own `catch` — or the
        // uncaught-exception path, which ends with a failing status anyway —
        // decides what happens next.
        return entry::undefined_value();
    };
    let reported = entry::make_number(f64::from(status));
    super::emit("exit", reported);
    // The streams here are unbuffered writes through `Write`, but a `console`
    // built on a line-buffered handle would otherwise lose its last line.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(status);
}

/// `process.abort()` — ends the process without unwinding or flushing, and
/// (where the platform allows) leaves a core dump.
pub(super) extern "C" fn abort(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let _ = std::io::stdout().flush();
    std::process::abort();
}

/// The status `exit` will end with, or `None` when the argument was refused and
/// a throw is already registered.
///
/// An explicit argument wins over `process.exitCode`, which is Node's rule. The
/// argument is read BEFORE anything else and no borrow is held across the exit.
///
/// The two paths are checked differently ON PURPOSE. An explicit argument goes
/// through `super::validate::exit_code`, which RAISES what Node raises. A
/// stored `exitCode` cannot: it is a plain data property (nothing in this crate
/// can install a setter — see the module doc's own note), so a bad value was
/// written long before `exit` was called and there is no call left to refuse.
/// `1` is what it becomes, because `0` would report SUCCESS for a program that
/// could not say what it meant, which is the worst shape a wrong answer takes.
fn status_for(code: u64) -> Option<i32> {
    let absent = entry::undefined_value();
    if code != absent {
        return super::validate::exit_code(code);
    }
    let stored = entry::with_runtime(|context| {
        entry::get_member(context, super::object(), "exitCode")
    });
    match stored == absent {
        true => Some(0),
        false => Some(integer_of(stored).unwrap_or(1)),
    }
}

/// A value read as an exit status or a signal number.
///
/// Two steps and not one: `number_of` answers only for values that really are
/// numbers, and Node documents this argument may also be a STRING. Reaching for
/// `text_of` instead would coerce anything at all — including `undefined`, and
/// including an object whose `toString` a native must not run — which is the
/// coercion-as-a-type-test defect this repository has already paid for three
/// times.
fn integer_of(value: u64) -> Option<i32> {
    if let Some(number) = entry::number_of(value) {
        return Some(number as i32);
    }
    let text = entry::with_runtime(|context| entry::string_in(context, value))?;
    text.trim().parse::<i32>().ok()
}

thread_local! {
    /// This thread's pending `nextTick` callbacks, in the order they were
    /// queued.
    ///
    /// Per thread for the reason [`crate::timers`]'s table is: a callback and
    /// its arguments are cells in the region of the thread that queued them, so
    /// a shared table would let one thread call another thread's function with
    /// the wrong context installed.
    static TICKS: RefCell<Vec<(u64, [u64; 3])>> = const { RefCell::new(Vec::new()) };
}

/// `process.nextTick(callback, ...args)` — up to three arguments, which is what
/// the four call slots leave once the callback takes one.
///
/// # WHEN it runs, which is not where Node runs it
///
/// Node drains the tick queue at a fixed point in the event loop, ahead of
/// Promise microtasks. This engine's modules reach the loop through
/// [`entry::declare_loop_source`], which is pumped after the program's last
/// statement and between waits — so a tick callback DOES run, and it runs after
/// the current script rather than interleaved ahead of the microtask queue.
/// That ordering is the divergence, stated here.
///
/// The alternative was this module's predecessor: refuse the member outright,
/// on the grounds that no tick hook existed. One does now, and a callback that
/// runs late is worth more than a `nextTick` that never runs — which is the
/// only other honest option, since registering and dropping is the shape this
/// crate refuses everywhere.
pub(super) extern "C" fn next_tick(
    _e: u64,
    _this: u64,
    callback: u64,
    a0: u64,
    a1: u64,
    a2: u64,
) -> u64 {
    let absent = entry::undefined_value();
    // A non-function argument is Node's `ERR_INVALID_ARG_TYPE`. Nothing is
    // queued rather than a value being called later — `is_callable_in` is the
    // test, because a truthiness check would queue a string.
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, callback));
    if callable {
        TICKS.with(|queue| queue.borrow_mut().push((callback, [a0, a1, a2])));
    }
    absent
}

/// Runs every callback queued so far.
///
/// The queue's borrow is released before any of them runs: a callback calling
/// `process.nextTick` again is ordinary, and it would otherwise panic on a
/// borrow still held — which in an `extern "C"` frame is an abort, not an
/// error. Anything queued during the drain waits for the next pass, matching
/// Node's own "the queue is drained, then re-checked" shape.
fn pump() {
    let due: Vec<(u64, [u64; 3])> = TICKS.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    let absent = entry::undefined_value();
    for (callback, arguments) in due {
        entry::call(callback, absent, arguments[0], arguments[1], arguments[2], absent);
    }
}

/// This module as a loop source.
///
/// `In(ZERO)` while anything is left, so the host comes straight back rather
/// than sleeping on work that is already due — a tick queued BY a tick is
/// Node's own starvation case and is reproduced rather than broken.
pub(super) fn source() -> entry::Pending {
    pump();
    let waiting = TICKS.with(|queue| !queue.borrow().is_empty());
    match waiting {
        true => entry::Pending::In(Duration::ZERO),
        false => entry::Pending::Idle,
    }
}

/// `process.emitWarning(warning[, type|options][, code])`.
///
/// Both call forms, distinguished by asking what the second argument IS rather
/// than by argument count: a string is Node's legacy `type`, an object is the
/// options bag. `ctor` (the fourth, stack-trimming argument) is not read —
/// there is no stack to trim here — and the four slots are spent by then.
///
/// With no `'warning'` listener the warning is printed the way Node prints it,
/// rather than discarded: a warning nobody sees is the same silence this module
/// refuses elsewhere.
pub(super) extern "C" fn emit_warning(
    _e: u64,
    _this: u64,
    warning: u64,
    options: u64,
    code: u64,
    _a3: u64,
) -> u64 {
    let absent = entry::undefined_value();
    let (kind, given_code, detail) = warning_options(options, code);
    let text = entry::with_runtime(|context| entry::string_in(context, warning));
    let value = match &text {
        // A string warning becomes the object a `'warning'` listener expects:
        // `name`, `message`, and the optional `code`/`detail`.
        Some(message) => entry::with_runtime(|context| {
            let object = entry::make_object(context);
            let name = entry::make_string(context, &kind);
            entry::put_member(context, object, "name", name);
            let held = entry::make_string(context, message);
            entry::put_member(context, object, "message", held);
            for (key, held) in [("code", &given_code), ("detail", &detail)] {
                if let Some(held) = held {
                    let value = entry::make_string(context, held);
                    entry::put_member(context, object, key, value);
                }
            }
            object
        }),
        // Anything else — in practice an `Error` — is passed through unchanged,
        // which is what Node does and what keeps `err.stack` intact.
        None => warning,
    };
    if !super::emit("warning", value) {
        let message = text
            .or_else(|| entry::described(value))
            .unwrap_or_else(|| "an object".to_owned());
        let code = given_code.map(|held| format!(" [{held}]")).unwrap_or_default();
        eprintln!("(rts:{}) {kind}: {message}{code}", std::process::id());
    }
    absent
}

/// `type`, `code` and `detail` out of `emitWarning`'s overloaded second and
/// third arguments.
fn warning_options(options: u64, code: u64) -> (String, Option<String>, Option<String>) {
    entry::with_runtime(|context| {
        let mut kind = entry::string_in(context, options);
        let mut given = entry::string_in(context, code);
        let mut detail = None;
        // Only when the second argument is neither a string nor absent: an
        // options bag. `is_object` is the test rather than "not a string",
        // because a number there is neither and must not be read as a bag.
        if kind.is_none() && entry::is_object(context, options) {
            // Each read is its own statement: `string_in` takes the context
            // shared and `get_member` takes it uniquely, so nesting the two in
            // one expression is a borrow conflict rather than a style choice.
            let named = entry::get_member(context, options, "type");
            kind = entry::string_in(context, named);
            if given.is_none() {
                let held = entry::get_member(context, options, "code");
                given = entry::string_in(context, held);
            }
            let held = entry::get_member(context, options, "detail");
            detail = entry::string_in(context, held);
        }
        (kind.unwrap_or_else(|| "Warning".to_owned()), given, detail)
    })
}

/// `process.kill(pid[, signal])`.
///
/// Despite the name this SENDS a signal: the default is `SIGTERM`, and signal
/// `0` is the standard existence-and-permission check. Node throws `ESRCH`/
/// `EPERM`; this answers `false` and reports on stderr, which is as close as a
/// crate that cannot throw gets, and is distinguishable from the `true` a
/// delivered signal answers.
#[cfg(unix)]
pub(super) extern "C" fn kill(
    _e: u64,
    _this: u64,
    pid: u64,
    signal: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(target) = super::validate::pid(pid) else {
        return entry::undefined_value();
    };
    let Some(number) = super::validate::signal(signal) else {
        return entry::undefined_value();
    };
    // SAFETY: `kill` reads two scalars and touches nothing this process owns.
    let answered = unsafe { libc::kill(target as libc::pid_t, number) };
    if answered != 0 {
        eprintln!("rts: process.kill: {}", std::io::Error::last_os_error());
    }
    entry::boolean_value(answered == 0)
}

/// Windows form of [`kill`]. Windows has no signal delivery — `OpenProcess`
/// with `PROCESS_QUERY_INFORMATION` is the existence/permission check signal
/// `0` asks for, and any non-zero signal (`SIGTERM` included: there is
/// nothing softer to ask a Windows process for) terminates via
/// `TerminateProcess`, the same substitution Node's own `libuv` backend
/// makes on this platform.
#[cfg(windows)]
pub(super) extern "C" fn kill(
    _e: u64,
    _this: u64,
    pid: u64,
    signal: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(target) = super::validate::pid(pid) else {
        return entry::undefined_value();
    };
    // Validated on this platform too, even though nothing is delivered: a
    // misspelled signal name is a bug in the program either way, and refusing
    // it only on POSIX would make the two platforms disagree about which
    // programs are correct.
    let Some(number) = super::validate::signal(signal) else {
        return entry::undefined_value();
    };
    // `0` is the explicit existence check; anything else — `SIGTERM` included,
    // this platform having no softer analogue — terminates.
    let is_probe = number == 0;
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn TerminateProcess(handle: isize, exit_code: u32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    let access = if is_probe { PROCESS_QUERY_INFORMATION } else { PROCESS_TERMINATE };
    // SAFETY: three Win32 calls over a `HANDLE` this function opens and
    // closes itself; no pointer beyond the handle value crosses the FFI
    // boundary.
    let answered = unsafe {
        let handle = OpenProcess(access, 0, target as u32);
        if handle == 0 {
            eprintln!("rts: process.kill: {}", std::io::Error::last_os_error());
            return entry::boolean_value(false);
        }
        let ok = if is_probe { 1 } else { TerminateProcess(handle, 1) };
        CloseHandle(handle);
        ok != 0
    };
    entry::boolean_value(answered)
}

/// The lock `umask`'s read needs — POSIX has no way to peek at the mask, only
/// to set it and be told the old one, so a reader must set-then-restore and two
/// threads doing that at once would leave the wrong mask installed.
#[cfg(unix)]
static UMASK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `process.umask([mask])` — POSIX only. Answers the PREVIOUS mask in both
/// forms, which is Node's contract.
#[cfg(unix)]
pub(super) extern "C" fn umask(
    _e: u64,
    _this: u64,
    mask: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    // Poisoning is not a reason to refuse: the guarded region is two syscalls
    // with no invariant a panic could have broken.
    let _guard = UMASK.lock().unwrap_or_else(|held| held.into_inner());
    let requested = integer_of(mask);
    // SAFETY: `umask` takes and returns a scalar.
    let previous = unsafe { libc::umask(requested.unwrap_or(0) as libc::mode_t) };
    if requested.is_none() {
        unsafe { libc::umask(previous) };
    }
    entry::make_number(previous as f64)
}

/// `process.umask([mask])` on Windows — always 0, which is what Node answers.
///
/// Not a stub, and the distinction is the one this module's header draws.
/// Windows has no file-mode creation mask, so nothing is masked, and 0 is the
/// true answer rather than a placeholder — Node on Windows answers exactly this
/// (checked: `process.umask()` is 0 before and after `process.umask(0o022)`).
/// A SET is accepted and does nothing, again as Node does, because refusing it
/// would break the line every Node test suite opens with.
///
/// It was absent here, and that cost the whole of Node's own corpus: its
/// `test/common/index.js` calls it unconditionally on the way in, so every
/// file died with `process.umask is not a function` before reaching its first
/// assertion.
#[cfg(not(unix))]
pub(super) extern "C" fn umask(
    _e: u64,
    _this: u64,
    _mask: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(0.0)
}

/// `process.getuid()` — POSIX only.
#[cfg(unix)]
pub(super) extern "C" fn getuid(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(f64::from(unsafe { libc::getuid() }))
}

/// `process.geteuid()` — POSIX only.
#[cfg(unix)]
pub(super) extern "C" fn geteuid(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(f64::from(unsafe { libc::geteuid() }))
}

/// `process.getgid()` — POSIX only.
#[cfg(unix)]
pub(super) extern "C" fn getgid(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(f64::from(unsafe { libc::getgid() }))
}

/// `process.getegid()` — POSIX only.
#[cfg(unix)]
pub(super) extern "C" fn getegid(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(f64::from(unsafe { libc::getegid() }))
}
