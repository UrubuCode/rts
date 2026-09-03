//! `util.getSystemErrorName` — libuv's negative errno enumeration.
//!
//! # Why this table and not `fs`'s or `wasi`'s
//!
//! `fs::io_node_code` answers a NAME from `std::io::ErrorKind`, with no number
//! anywhere — nothing to reuse a number FROM. `wasi::errno` is a different
//! enumeration entirely (WASI preview1's own spec-assigned positive codes,
//! `EBADF` = 8 there against libuv's `-4083` here); sharing it would mean two
//! unrelated specifications happening to use the same C name, not one number
//! reused. Searched both in full before writing this, per `reuse-check`: there
//! is no existing numbering to reuse, only names that coincide.
//!
//! # Why the table is split by platform rather than written once
//!
//! libuv does not give `UV_ENOENT` one number. On POSIX it is
//! `#define UV_ENOENT (-ENOENT)` — literally the negative of whatever the
//! platform's own `<errno.h>` assigns, which is what makes the POSIX half of
//! this module a FORMULA (`-libc::E*`) rather than a second copy of a number:
//! `libc::ENOENT` already carries the value the target's own headers declare
//! it, cross-compiled per target automatically. On Windows there is no
//! `<errno.h>` entry to negate, so libuv hardcodes a synthetic value instead —
//! a table, not a formula, and the one below was not typed from documentation
//! but read back from a real `util.getSystemErrorName` call, once per code
//! from -1 to -4200, against Node.js v20.19.5 on this machine. That scan is
//! also what caught two divergences from libuv's own `include/uv/errno.h`
//! text: `UV_ENOEXEC` and `UV_EHOSTDOWN` are macros in that header but this
//! Windows build's `uv_err_name` does not answer either one, so both are
//! correctly ABSENT below rather than carried over from a document this
//! binary disagrees with.
//!
//! # Why seven POSIX names are missing from the Unix half
//!
//! `ECHARSET`, `ENONET`, `EREMOTEIO`, `EUNATCH`, `ENODATA`,
//! `ESOCKTNOSUPPORT` and `EFTYPE` are not portable the way the other 77 are:
//! each is either not a real errno on every Unix `libc` targets this engine
//! could build for (`EFTYPE` is BSD/macOS only; `ENONET`, `EREMOTEIO` and
//! `EUNATCH` are Linux-only) or not a real errno anywhere (`ECHARSET` is
//! libuv's own invention, with no system definition to negate). A name typed
//! in from one platform's headers would compile on that platform and be
//! silently wrong — or refuse to compile at all — on another, which is the
//! same trade [`super::signals::SIGNALS`] states for the Unix signal numbers
//! it likewise declines to guess at. Unreachable here, these seven fall
//! through to the same "Unknown system error N" a genuinely unassigned number
//! gets — an honest answer instead of a fabricated one.
//!
//! The four-digit numbers for `EAI_*`, `UNKNOWN` and `EOF` are NOT this
//! formula: libuv assigns them the same literal value on every platform (no
//! `#define UV_EAI_ADDRFAMILY (-EAI_ADDRFAMILY)` exists anywhere in its
//! source), confirmed against `include/uv/errno.h` and against this Windows
//! scan agreeing with it, so they are written once and shared by both
//! `#[cfg]` arms below.

use rts_core::entry;

use super::values::number_of;

/// Numbers libuv assigns the SAME way on every platform: the `getaddrinfo`
/// family (`-3000..-3014`, with `-3012` never assigned, on both this scan and
/// the published header) and the two catch-alls at the bottom of the range.
const SHARED: &[(i32, &str)] = &[
    (-3000, "EAI_ADDRFAMILY"),
    (-3001, "EAI_AGAIN"),
    (-3002, "EAI_BADFLAGS"),
    (-3003, "EAI_CANCELED"),
    (-3004, "EAI_FAIL"),
    (-3005, "EAI_FAMILY"),
    (-3006, "EAI_MEMORY"),
    (-3007, "EAI_NODATA"),
    (-3008, "EAI_NONAME"),
    (-3009, "EAI_OVERFLOW"),
    (-3010, "EAI_SERVICE"),
    (-3011, "EAI_SOCKTYPE"),
    (-3013, "EAI_BADHINTS"),
    (-3014, "EAI_PROTOCOL"),
    (-4094, "UNKNOWN"),
    (-4095, "EOF"),
];

/// The Windows enumeration, read back from a real `util.getSystemErrorName`
/// call for every code from -1 to -4200 against Node.js v20.19.5 on this
/// machine — see the module doc for why that scan and not the published
/// header is what this table transcribes.
#[cfg(windows)]
const PLATFORM: &[(i32, &str)] = &[
    (-4023, "EUNATCH"),
    (-4024, "ENODATA"),
    (-4025, "ESOCKTNOSUPPORT"),
    (-4026, "EOVERFLOW"),
    (-4027, "EILSEQ"),
    (-4028, "EFTYPE"),
    (-4029, "ENOTTY"),
    (-4030, "EREMOTEIO"),
    (-4031, "EHOSTDOWN"),
    (-4032, "EMLINK"),
    (-4033, "ENXIO"),
    (-4034, "ERANGE"),
    (-4035, "ENOPROTOOPT"),
    (-4036, "EFBIG"),
    (-4037, "EXDEV"),
    (-4038, "ETXTBSY"),
    (-4039, "ETIMEDOUT"),
    (-4040, "ESRCH"),
    (-4041, "ESPIPE"),
    (-4042, "ESHUTDOWN"),
    (-4043, "EROFS"),
    (-4044, "EPROTOTYPE"),
    (-4045, "EPROTONOSUPPORT"),
    (-4046, "EPROTO"),
    (-4047, "EPIPE"),
    (-4048, "EPERM"),
    (-4049, "ENOTSUP"),
    (-4050, "ENOTSOCK"),
    (-4051, "ENOTEMPTY"),
    (-4052, "ENOTDIR"),
    (-4053, "ENOTCONN"),
    (-4054, "ENOSYS"),
    (-4055, "ENOSPC"),
    (-4056, "ENONET"),
    (-4057, "ENOMEM"),
    (-4058, "ENOENT"),
    (-4059, "ENODEV"),
    (-4060, "ENOBUFS"),
    (-4061, "ENFILE"),
    (-4062, "ENETUNREACH"),
    (-4063, "ENETDOWN"),
    (-4064, "ENAMETOOLONG"),
    (-4065, "EMSGSIZE"),
    (-4066, "EMFILE"),
    (-4067, "ELOOP"),
    (-4068, "EISDIR"),
    (-4069, "EISCONN"),
    (-4070, "EIO"),
    (-4071, "EINVAL"),
    (-4072, "EINTR"),
    (-4073, "EHOSTUNREACH"),
    (-4074, "EFAULT"),
    (-4075, "EEXIST"),
    (-4076, "EDESTADDRREQ"),
    (-4077, "ECONNRESET"),
    (-4078, "ECONNREFUSED"),
    (-4079, "ECONNABORTED"),
    (-4080, "ECHARSET"),
    (-4081, "ECANCELED"),
    (-4082, "EBUSY"),
    (-4083, "EBADF"),
    (-4084, "EALREADY"),
    (-4088, "EAGAIN"),
    (-4089, "EAFNOSUPPORT"),
    (-4090, "EADDRNOTAVAIL"),
    (-4091, "EADDRINUSE"),
    (-4092, "EACCES"),
    (-4093, "E2BIG"),
];

/// The Unix half of libuv's enumeration: `-libc::E*`, the same formula libuv's
/// own `UV__ERR(x)` macro applies — see the module doc for the seven names
/// this leaves out and why.
#[cfg(unix)]
const PLATFORM: &[(i32, &str)] = &[
    (-(libc::E2BIG as i32), "E2BIG"),
    (-(libc::EACCES as i32), "EACCES"),
    (-(libc::EADDRINUSE as i32), "EADDRINUSE"),
    (-(libc::EADDRNOTAVAIL as i32), "EADDRNOTAVAIL"),
    (-(libc::EAFNOSUPPORT as i32), "EAFNOSUPPORT"),
    (-(libc::EAGAIN as i32), "EAGAIN"),
    (-(libc::EALREADY as i32), "EALREADY"),
    (-(libc::EBADF as i32), "EBADF"),
    (-(libc::EBUSY as i32), "EBUSY"),
    (-(libc::ECANCELED as i32), "ECANCELED"),
    (-(libc::ECONNABORTED as i32), "ECONNABORTED"),
    (-(libc::ECONNREFUSED as i32), "ECONNREFUSED"),
    (-(libc::ECONNRESET as i32), "ECONNRESET"),
    (-(libc::EDESTADDRREQ as i32), "EDESTADDRREQ"),
    (-(libc::EEXIST as i32), "EEXIST"),
    (-(libc::EFAULT as i32), "EFAULT"),
    (-(libc::EFBIG as i32), "EFBIG"),
    (-(libc::EHOSTDOWN as i32), "EHOSTDOWN"),
    (-(libc::EHOSTUNREACH as i32), "EHOSTUNREACH"),
    (-(libc::EILSEQ as i32), "EILSEQ"),
    (-(libc::EINTR as i32), "EINTR"),
    (-(libc::EINVAL as i32), "EINVAL"),
    (-(libc::EIO as i32), "EIO"),
    (-(libc::EISCONN as i32), "EISCONN"),
    (-(libc::EISDIR as i32), "EISDIR"),
    (-(libc::ELOOP as i32), "ELOOP"),
    (-(libc::EMFILE as i32), "EMFILE"),
    (-(libc::EMLINK as i32), "EMLINK"),
    (-(libc::EMSGSIZE as i32), "EMSGSIZE"),
    (-(libc::ENAMETOOLONG as i32), "ENAMETOOLONG"),
    (-(libc::ENETDOWN as i32), "ENETDOWN"),
    (-(libc::ENETUNREACH as i32), "ENETUNREACH"),
    (-(libc::ENFILE as i32), "ENFILE"),
    (-(libc::ENOBUFS as i32), "ENOBUFS"),
    (-(libc::ENODEV as i32), "ENODEV"),
    (-(libc::ENOENT as i32), "ENOENT"),
    (-(libc::ENOMEM as i32), "ENOMEM"),
    (-(libc::ENOPROTOOPT as i32), "ENOPROTOOPT"),
    (-(libc::ENOSPC as i32), "ENOSPC"),
    (-(libc::ENOSYS as i32), "ENOSYS"),
    (-(libc::ENOTCONN as i32), "ENOTCONN"),
    (-(libc::ENOTDIR as i32), "ENOTDIR"),
    (-(libc::ENOTEMPTY as i32), "ENOTEMPTY"),
    (-(libc::ENOTSOCK as i32), "ENOTSOCK"),
    (-(libc::ENOTSUP as i32), "ENOTSUP"),
    (-(libc::ENOTTY as i32), "ENOTTY"),
    (-(libc::ENXIO as i32), "ENXIO"),
    (-(libc::EOVERFLOW as i32), "EOVERFLOW"),
    (-(libc::EPERM as i32), "EPERM"),
    (-(libc::EPIPE as i32), "EPIPE"),
    (-(libc::EPROTO as i32), "EPROTO"),
    (-(libc::EPROTONOSUPPORT as i32), "EPROTONOSUPPORT"),
    (-(libc::EPROTOTYPE as i32), "EPROTOTYPE"),
    (-(libc::ERANGE as i32), "ERANGE"),
    (-(libc::EROFS as i32), "EROFS"),
    (-(libc::ESHUTDOWN as i32), "ESHUTDOWN"),
    (-(libc::ESPIPE as i32), "ESPIPE"),
    (-(libc::ESRCH as i32), "ESRCH"),
    (-(libc::ETIMEDOUT as i32), "ETIMEDOUT"),
    (-(libc::ETXTBSY as i32), "ETXTBSY"),
    (-(libc::EXDEV as i32), "EXDEV"),
];

/// The name for a negative errno, or `None` when nothing claims it.
pub(super) fn name_for(errno: i32) -> Option<&'static str> {
    SHARED
        .iter()
        .chain(PLATFORM.iter())
        .find(|(number, _)| *number == errno)
        .map(|(_, name)| *name)
}

/// `util.getSystemErrorName(err)`.
///
/// Validation matches Node's own order, checked against Node.js v20.19.5:
/// wrong type raises `TypeError [ERR_INVALID_ARG_TYPE]` before the range is
/// even considered, then a number that is not a negative integer raises
/// `RangeError [ERR_OUT_OF_RANGE]` — `-0` included, since `-0 < 0` is false.
/// An in-range negative integer this table does not cover answers Node's own
/// `"Unknown system error {n}"` string rather than `undefined`, which is what
/// makes that case distinguishable from the two throws above.
pub(super) extern "C" fn get_system_error_name(
    _e: u64,
    _this: u64,
    err: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(number) = number_of(err) else {
        entry::invalid_arg_type("err", "number", err);
        return entry::undefined_value();
    };
    if !(number.fract() == 0.0 && number < 0.0) {
        entry::out_of_range("err", "a negative integer", err);
        return entry::undefined_value();
    }
    let code = number as i64;
    let text = match name_for(code as i32) {
        Some(name) => name.to_owned(),
        None => format!("Unknown system error {code}"),
    };
    entry::with_runtime(|context| entry::make_string(context, &text))
}
