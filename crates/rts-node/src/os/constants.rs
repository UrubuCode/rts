//! `os.constants` — the five tables, assembled per target at COMPILE time.
//!
//! # Why a key is absent rather than zero
//!
//! Node's own rule, and the honesty floor's: `SIGBREAK` does not exist on
//! POSIX and the `WSA*` family does not exist off Windows, so the KEY is
//! missing there rather than present with a `0`. A program feature-detecting
//! `if (os.constants.signals.SIGBREAK)` must find nothing, and a zero would
//! read as "this signal is number zero".
//!
//! # Where the numbers come from
//!
//! `libc`, wherever `libc` defines them — including on Windows, whose module
//! carries the MSVC CRT's `errno` and signal values. That is the point of
//! sourcing them rather than typing them: `EWOULDBLOCK` is 140 on the Windows
//! CRT and 11 on Linux, and a hand-written table would have one of them wrong.
//!
//! Two groups are NOT the host's and are written here: `priority` and `libuv`
//! are Node-level conventions with no OS behind them (`PRIORITY_LOW` is 19 on
//! every platform Node runs on), and the `WSA*` values, which are a published
//! fixed ABI (`WSABASEERR` + a documented offset) that the `libc` Windows
//! module does not carry.

use rts_core::entry::Context;

/// The `os.constants` object, built once — see `mod.rs` on why once.
pub(super) fn object(context: &mut Context) -> u64 {
    let root = rts_core::entry::make_object(context);
    for (name, entries) in [
        ("signals", SIGNALS),
        ("errno", ERRNO),
        ("dlopen", DLOPEN),
        ("priority", PRIORITY),
        ("libuv", LIBUV),
    ] {
        let group = rts_core::entry::make_object(context);
        for (key, value) in entries {
            let number = rts_core::entry::make_number(f64::from(*value));
            rts_core::entry::put_member(context, group, key, number);
        }
        rts_core::entry::put_member(context, root, name, group);
    }
    // `errno` gains the Winsock family on Windows only — a second write into
    // the group already built rather than a second table, so there is one
    // answer to what `os.constants.errno` IS.
    #[cfg(windows)]
    {
        let errno = rts_core::entry::get_member(context, root, "errno");
        for (key, value) in WINSOCK {
            let number = rts_core::entry::make_number(f64::from(*value));
            rts_core::entry::put_member(context, errno, key, number);
        }
    }
    root
}

/// Node's own six-bucket scale, identical on every platform.
static PRIORITY: &[(&str, i32)] = &[
    ("PRIORITY_LOW", 19),
    ("PRIORITY_BELOW_NORMAL", 10),
    ("PRIORITY_NORMAL", 0),
    ("PRIORITY_ABOVE_NORMAL", -7),
    ("PRIORITY_HIGH", -14),
    ("PRIORITY_HIGHEST", -20),
];

/// The one libuv flag Node mirrors here. `node:dgram` is what acts on it.
static LIBUV: &[(&str, i32)] = &[("UV_UDP_REUSEADDR", 4)];

/// The MSVC CRT's signal set — the whole of it. Windows has no `SIGHUP`,
/// `SIGKILL` or job-control signals at all, so they are absent as keys.
///
/// `SIGBREAK` is the one value not sourced from `libc`, whose Windows module
/// omits it; `21` is the CRT's published `signal.h` number, which is what Node
/// reports and what `GenerateConsoleCtrlEvent` delivers.
#[cfg(windows)]
pub(crate) static SIGNALS: &[(&str, i32)] = &[
    ("SIGINT", libc::SIGINT),
    ("SIGILL", libc::SIGILL),
    ("SIGFPE", libc::SIGFPE),
    ("SIGSEGV", libc::SIGSEGV),
    ("SIGTERM", libc::SIGTERM),
    ("SIGBREAK", 21),
    ("SIGABRT", libc::SIGABRT),
];

/// The POSIX signals every target RTS builds for defines.
///
/// `SIGSTKFLT`, `SIGPWR` and `SIGPOLL` are Linux-only and appear below;
/// `SIGINFO`, `SIGLOST`, `SIGUNUSED` and `SIGIOT` are absent because they do
/// not exist on both Linux and Darwin, and a key present on one platform with
/// another's number is exactly the fabrication this file refuses.
#[cfg(not(windows))]
pub(crate) static SIGNALS: &[(&str, i32)] = &[
    ("SIGHUP", libc::SIGHUP),
    ("SIGINT", libc::SIGINT),
    ("SIGQUIT", libc::SIGQUIT),
    ("SIGILL", libc::SIGILL),
    ("SIGTRAP", libc::SIGTRAP),
    ("SIGABRT", libc::SIGABRT),
    ("SIGBUS", libc::SIGBUS),
    ("SIGFPE", libc::SIGFPE),
    ("SIGKILL", libc::SIGKILL),
    ("SIGUSR1", libc::SIGUSR1),
    ("SIGUSR2", libc::SIGUSR2),
    ("SIGSEGV", libc::SIGSEGV),
    ("SIGPIPE", libc::SIGPIPE),
    ("SIGALRM", libc::SIGALRM),
    ("SIGTERM", libc::SIGTERM),
    ("SIGCHLD", libc::SIGCHLD),
    ("SIGCONT", libc::SIGCONT),
    ("SIGSTOP", libc::SIGSTOP),
    ("SIGTSTP", libc::SIGTSTP),
    ("SIGTTIN", libc::SIGTTIN),
    ("SIGTTOU", libc::SIGTTOU),
    ("SIGURG", libc::SIGURG),
    ("SIGXCPU", libc::SIGXCPU),
    ("SIGXFSZ", libc::SIGXFSZ),
    ("SIGVTALRM", libc::SIGVTALRM),
    ("SIGPROF", libc::SIGPROF),
    ("SIGWINCH", libc::SIGWINCH),
    ("SIGIO", libc::SIGIO),
    ("SIGSYS", libc::SIGSYS),
    #[cfg(target_os = "linux")]
    ("SIGSTKFLT", libc::SIGSTKFLT),
    #[cfg(target_os = "linux")]
    ("SIGPWR", libc::SIGPWR),
    #[cfg(target_os = "linux")]
    ("SIGPOLL", libc::SIGPOLL),
];

/// `dlopen(3)` flags. Empty on Windows, which is parity: there is no
/// `dlopen(3)` there and Node's object is empty too.
#[cfg(windows)]
pub(crate) static DLOPEN: &[(&str, i32)] = &[];

#[cfg(not(windows))]
pub(crate) static DLOPEN: &[(&str, i32)] = &[
    ("RTLD_LAZY", libc::RTLD_LAZY),
    ("RTLD_NOW", libc::RTLD_NOW),
    ("RTLD_GLOBAL", libc::RTLD_GLOBAL),
    ("RTLD_LOCAL", libc::RTLD_LOCAL),
    #[cfg(target_os = "linux")]
    ("RTLD_DEEPBIND", libc::RTLD_DEEPBIND),
];

/// The MSVC CRT's `errno` set, as `libc` declares it.
///
/// `EDQUOT`, `EMULTIHOP` and `ESTALE` are absent: the CRT has no such values,
/// and Node does not report them on Windows either.
#[cfg(windows)]
pub(crate) static ERRNO: &[(&str, i32)] = &[
    ("E2BIG", libc::E2BIG),
    ("EACCES", libc::EACCES),
    ("EADDRINUSE", libc::EADDRINUSE),
    ("EADDRNOTAVAIL", libc::EADDRNOTAVAIL),
    ("EAFNOSUPPORT", libc::EAFNOSUPPORT),
    ("EAGAIN", libc::EAGAIN),
    ("EALREADY", libc::EALREADY),
    ("EBADF", libc::EBADF),
    ("EBADMSG", libc::EBADMSG),
    ("EBUSY", libc::EBUSY),
    ("ECANCELED", libc::ECANCELED),
    ("ECHILD", libc::ECHILD),
    ("ECONNABORTED", libc::ECONNABORTED),
    ("ECONNREFUSED", libc::ECONNREFUSED),
    ("ECONNRESET", libc::ECONNRESET),
    ("EDEADLK", libc::EDEADLK),
    ("EDESTADDRREQ", libc::EDESTADDRREQ),
    ("EDOM", libc::EDOM),
    ("EEXIST", libc::EEXIST),
    ("EFAULT", libc::EFAULT),
    ("EFBIG", libc::EFBIG),
    ("EHOSTUNREACH", libc::EHOSTUNREACH),
    ("EIDRM", libc::EIDRM),
    ("EILSEQ", libc::EILSEQ),
    ("EINPROGRESS", libc::EINPROGRESS),
    ("EINTR", libc::EINTR),
    ("EINVAL", libc::EINVAL),
    ("EIO", libc::EIO),
    ("EISCONN", libc::EISCONN),
    ("EISDIR", libc::EISDIR),
    ("ELOOP", libc::ELOOP),
    ("EMFILE", libc::EMFILE),
    ("EMLINK", libc::EMLINK),
    ("EMSGSIZE", libc::EMSGSIZE),
    ("ENAMETOOLONG", libc::ENAMETOOLONG),
    ("ENETDOWN", libc::ENETDOWN),
    ("ENETRESET", libc::ENETRESET),
    ("ENETUNREACH", libc::ENETUNREACH),
    ("ENFILE", libc::ENFILE),
    ("ENOBUFS", libc::ENOBUFS),
    ("ENODATA", libc::ENODATA),
    ("ENODEV", libc::ENODEV),
    ("ENOENT", libc::ENOENT),
    ("ENOEXEC", libc::ENOEXEC),
    ("ENOLCK", libc::ENOLCK),
    ("ENOLINK", libc::ENOLINK),
    ("ENOMEM", libc::ENOMEM),
    ("ENOMSG", libc::ENOMSG),
    ("ENOPROTOOPT", libc::ENOPROTOOPT),
    ("ENOSPC", libc::ENOSPC),
    ("ENOSR", libc::ENOSR),
    ("ENOSTR", libc::ENOSTR),
    ("ENOSYS", libc::ENOSYS),
    ("ENOTCONN", libc::ENOTCONN),
    ("ENOTDIR", libc::ENOTDIR),
    ("ENOTEMPTY", libc::ENOTEMPTY),
    ("ENOTSOCK", libc::ENOTSOCK),
    ("ENOTSUP", libc::ENOTSUP),
    ("ENOTTY", libc::ENOTTY),
    ("ENXIO", libc::ENXIO),
    ("EOPNOTSUPP", libc::EOPNOTSUPP),
    ("EOVERFLOW", libc::EOVERFLOW),
    ("EPERM", libc::EPERM),
    ("EPIPE", libc::EPIPE),
    ("EPROTO", libc::EPROTO),
    ("EPROTONOSUPPORT", libc::EPROTONOSUPPORT),
    ("EPROTOTYPE", libc::EPROTOTYPE),
    ("ERANGE", libc::ERANGE),
    ("EROFS", libc::EROFS),
    ("ESPIPE", libc::ESPIPE),
    ("ESRCH", libc::ESRCH),
    ("ETIME", libc::ETIME),
    ("ETIMEDOUT", libc::ETIMEDOUT),
    ("ETXTBSY", libc::ETXTBSY),
    ("EWOULDBLOCK", libc::EWOULDBLOCK),
    ("EXDEV", libc::EXDEV),
];

/// The POSIX `errno` set every target RTS builds for defines.
///
/// The four STREAMS values (`ENODATA`, `ENOSR`, `ENOSTR`, `ETIME`) and
/// `EMULTIHOP` are Linux-gated: they are legacy XSI and not present under that
/// name on every Unix `libc` binds, and a key with the wrong number is worse
/// than an absent one.
#[cfg(not(windows))]
pub(crate) static ERRNO: &[(&str, i32)] = &[
    ("E2BIG", libc::E2BIG),
    ("EACCES", libc::EACCES),
    ("EADDRINUSE", libc::EADDRINUSE),
    ("EADDRNOTAVAIL", libc::EADDRNOTAVAIL),
    ("EAFNOSUPPORT", libc::EAFNOSUPPORT),
    ("EAGAIN", libc::EAGAIN),
    ("EALREADY", libc::EALREADY),
    ("EBADF", libc::EBADF),
    ("EBADMSG", libc::EBADMSG),
    ("EBUSY", libc::EBUSY),
    ("ECANCELED", libc::ECANCELED),
    ("ECHILD", libc::ECHILD),
    ("ECONNABORTED", libc::ECONNABORTED),
    ("ECONNREFUSED", libc::ECONNREFUSED),
    ("ECONNRESET", libc::ECONNRESET),
    ("EDEADLK", libc::EDEADLK),
    ("EDESTADDRREQ", libc::EDESTADDRREQ),
    ("EDOM", libc::EDOM),
    ("EDQUOT", libc::EDQUOT),
    ("EEXIST", libc::EEXIST),
    ("EFAULT", libc::EFAULT),
    ("EFBIG", libc::EFBIG),
    ("EHOSTUNREACH", libc::EHOSTUNREACH),
    ("EIDRM", libc::EIDRM),
    ("EILSEQ", libc::EILSEQ),
    ("EINPROGRESS", libc::EINPROGRESS),
    ("EINTR", libc::EINTR),
    ("EINVAL", libc::EINVAL),
    ("EIO", libc::EIO),
    ("EISCONN", libc::EISCONN),
    ("EISDIR", libc::EISDIR),
    ("ELOOP", libc::ELOOP),
    ("EMFILE", libc::EMFILE),
    ("EMLINK", libc::EMLINK),
    ("EMSGSIZE", libc::EMSGSIZE),
    ("ENAMETOOLONG", libc::ENAMETOOLONG),
    ("ENETDOWN", libc::ENETDOWN),
    ("ENETRESET", libc::ENETRESET),
    ("ENETUNREACH", libc::ENETUNREACH),
    ("ENFILE", libc::ENFILE),
    ("ENOBUFS", libc::ENOBUFS),
    ("ENODEV", libc::ENODEV),
    ("ENOENT", libc::ENOENT),
    ("ENOEXEC", libc::ENOEXEC),
    ("ENOLCK", libc::ENOLCK),
    ("ENOLINK", libc::ENOLINK),
    ("ENOMEM", libc::ENOMEM),
    ("ENOMSG", libc::ENOMSG),
    ("ENOPROTOOPT", libc::ENOPROTOOPT),
    ("ENOSPC", libc::ENOSPC),
    ("ENOSYS", libc::ENOSYS),
    ("ENOTCONN", libc::ENOTCONN),
    ("ENOTDIR", libc::ENOTDIR),
    ("ENOTEMPTY", libc::ENOTEMPTY),
    ("ENOTSOCK", libc::ENOTSOCK),
    ("ENOTSUP", libc::ENOTSUP),
    ("ENOTTY", libc::ENOTTY),
    ("ENXIO", libc::ENXIO),
    ("EOPNOTSUPP", libc::EOPNOTSUPP),
    ("EOVERFLOW", libc::EOVERFLOW),
    ("EPERM", libc::EPERM),
    ("EPIPE", libc::EPIPE),
    ("EPROTO", libc::EPROTO),
    ("EPROTONOSUPPORT", libc::EPROTONOSUPPORT),
    ("EPROTOTYPE", libc::EPROTOTYPE),
    ("ERANGE", libc::ERANGE),
    ("EROFS", libc::EROFS),
    ("ESPIPE", libc::ESPIPE),
    ("ESRCH", libc::ESRCH),
    ("ESTALE", libc::ESTALE),
    ("ETIMEDOUT", libc::ETIMEDOUT),
    ("ETXTBSY", libc::ETXTBSY),
    ("EWOULDBLOCK", libc::EWOULDBLOCK),
    ("EXDEV", libc::EXDEV),
    #[cfg(target_os = "linux")]
    ("EMULTIHOP", libc::EMULTIHOP),
    #[cfg(target_os = "linux")]
    ("ENODATA", libc::ENODATA),
    #[cfg(target_os = "linux")]
    ("ENOSR", libc::ENOSR),
    #[cfg(target_os = "linux")]
    ("ENOSTR", libc::ENOSTR),
    #[cfg(target_os = "linux")]
    ("ETIME", libc::ETIME),
];

/// The Winsock error family, `WSABASEERR` (10000) plus the offsets
/// `winerror.h` publishes. A fixed ABI: a compiled program links against these
/// numbers, so they cannot drift the way a platform `errno` can.
#[cfg(windows)]
pub(crate) static WINSOCK: &[(&str, i32)] = &[
    ("WSAEINTR", 10004),
    ("WSAEBADF", 10009),
    ("WSAEACCES", 10013),
    ("WSAEFAULT", 10014),
    ("WSAEINVAL", 10022),
    ("WSAEMFILE", 10024),
    ("WSAEWOULDBLOCK", 10035),
    ("WSAEINPROGRESS", 10036),
    ("WSAEALREADY", 10037),
    ("WSAENOTSOCK", 10038),
    ("WSAEDESTADDRREQ", 10039),
    ("WSAEMSGSIZE", 10040),
    ("WSAEPROTOTYPE", 10041),
    ("WSAENOPROTOOPT", 10042),
    ("WSAEPROTONOSUPPORT", 10043),
    ("WSAESOCKTNOSUPPORT", 10044),
    ("WSAEOPNOTSUPP", 10045),
    ("WSAEPFNOSUPPORT", 10046),
    ("WSAEAFNOSUPPORT", 10047),
    ("WSAEADDRINUSE", 10048),
    ("WSAEADDRNOTAVAIL", 10049),
    ("WSAENETDOWN", 10050),
    ("WSAENETUNREACH", 10051),
    ("WSAENETRESET", 10052),
    ("WSAECONNABORTED", 10053),
    ("WSAECONNRESET", 10054),
    ("WSAENOBUFS", 10055),
    ("WSAEISCONN", 10056),
    ("WSAENOTCONN", 10057),
    ("WSAESHUTDOWN", 10058),
    ("WSAETOOMANYREFS", 10059),
    ("WSAETIMEDOUT", 10060),
    ("WSAECONNREFUSED", 10061),
    ("WSAELOOP", 10062),
    ("WSAENAMETOOLONG", 10063),
    ("WSAEHOSTDOWN", 10064),
    ("WSAEHOSTUNREACH", 10065),
    ("WSAENOTEMPTY", 10066),
    ("WSAEPROCLIM", 10067),
    ("WSAEUSERS", 10068),
    ("WSAEDQUOT", 10069),
    ("WSAESTALE", 10070),
    ("WSAEREMOTE", 10071),
    ("WSASYSNOTREADY", 10091),
    ("WSAVERNOTSUPPORTED", 10092),
    ("WSANOTINITIALISED", 10093),
    ("WSAEDISCON", 10101),
    ("WSAENOMORE", 10102),
    ("WSAECANCELLED", 10103),
    ("WSAEINVALIDPROCTABLE", 10104),
    ("WSAEINVALIDPROVIDER", 10105),
    ("WSAEPROVIDERFAILEDINIT", 10106),
    ("WSASYSCALLFAILURE", 10107),
    ("WSASERVICE_NOT_FOUND", 10108),
    ("WSATYPE_NOT_FOUND", 10109),
    ("WSA_E_NO_MORE", 10110),
    ("WSA_E_CANCELLED", 10111),
    ("WSAEREFUSED", 10112),
];
