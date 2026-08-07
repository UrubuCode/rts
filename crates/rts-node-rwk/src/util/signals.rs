//! `util.convertProcessSignalToExitCode` — the POSIX `128 + n` convention.

use rts_core_rwk::entry;

use super::values::string_of;

/// The signal numbers that are the SAME on every Unix this could run on.
///
/// # Why the list stops where it does
///
/// `SIGUSR1` is 10 on Linux and 30 on macOS; `SIGCHLD`, `SIGBUS`, `SIGCONT`,
/// `SIGSTOP`, `SIGTSTP`, `SIGURG` and `SIGWINCH` all differ likewise. A table
/// containing them would be right on the machine it was written on and quietly
/// wrong elsewhere — an exit code that names the wrong signal, which is a wrong
/// answer that runs. The twelve below are fixed by history across Linux, the
/// BSDs and macOS alike, and everything else answers the documented `0`.
const SIGNALS: &[(&str, u8)] = &[
    ("SIGHUP", 1),
    ("SIGINT", 2),
    ("SIGQUIT", 3),
    ("SIGILL", 4),
    ("SIGTRAP", 5),
    ("SIGABRT", 6),
    ("SIGIOT", 6),
    ("SIGFPE", 8),
    ("SIGKILL", 9),
    ("SIGSEGV", 11),
    ("SIGPIPE", 13),
    ("SIGALRM", 14),
    ("SIGTERM", 15),
];

/// `util.convertProcessSignalToExitCode(signal)`.
pub(super) extern "C" fn convert_signal(
    _e: u64,
    _this: u64,
    signal: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    // `string_in` and not `text_of`: this asks WHICH signal, and a coercion
    // would turn the number `9` into `"9"` and then answer `0` for it — the
    // shape of the three defects this crate's doc names.
    let named = string_of(signal)
        .and_then(|name| SIGNALS.iter().find(|(held, _)| *held == name).map(|(_, number)| *number));
    entry::make_number(match named {
        Some(number) => 128.0 + f64::from(number),
        None => 0.0,
    })
}
