//! `node:constants` — the deprecated FLAT view of three other modules' tables.
//!
//! # What it is
//!
//! Node's `constants` module (DEP0008) predates `os.constants`,
//! `fs.constants` and `crypto.constants` and is exactly their union with the
//! grouping removed:
//!
//! ```js
//! module.exports = { ...os.dlopen, ...os.errno, ...os.signals, ...fs, ...crypto };
//! ```
//!
//! So `constants.O_RDONLY`, `constants.SIGINT` and `constants.ENOENT` are all
//! top-level names here, and there is no `signals` or `errno` sub-object — a
//! program wanting those grouped is asking `node:os`, which is the whole reason
//! this module was deprecated.
//!
//! The deprecation is documentation-only in Node: importing it prints nothing
//! and returns a working object. This does the same, because a warning nothing
//! in Node emits would be this crate inventing behaviour rather than matching
//! it.
//!
//! # Reuse-check
//!
//! Every number here **already exists in this crate** and none of it is
//! restated:
//!
//! - `os::constants::{DLOPEN, ERRNO, SIGNALS, WINSOCK}` — the same statics
//!   [`crate::os`] builds `os.constants` from, sourced from `libc` per target.
//!   They were private and are now `pub(crate)`, which is the cheaper half of
//!   the choice: the alternative was reading them back out of the already-built
//!   `os.constants` object, which needs an own-key enumeration the entry API
//!   does not offer to a host crate, and which would have made this module
//!   depend on `node:os` being registered FIRST — an ordering `lib.rs` would
//!   then have to hold by hand, the way it already has to for `EventEmitter`.
//! - `fs::constants::ENTRIES` — the access/open/copy/mode bits, likewise.
//! - `crypto::CONSTANTS` — the RSA padding and PSS salt-length numbers, lifted
//!   out of `crypto`'s private builder for this reader.
//!
//! Nothing in `rts-cranelift` or `rts-core` answers "what number is
//! `ENOENT`" — these are OS facts, not machine bookkeeping — so the search
//! stopped inside this crate, which is where the three tables were.
//!
//! Because the tables are shared rather than copied,
//! `constants.ENOENT === os.constants.errno.ENOENT` holds by construction and
//! cannot drift.
//!
//! # Which platform's numbers
//!
//! The host's, decided at COMPILE time by the tables above — on this Windows
//! build that means the MSVC CRT's `errno` and signal values plus the Winsock
//! family, not Linux's. `EWOULDBLOCK` is 140 here and 11 on Linux, and that
//! difference is the reason this module reaches for `libc` through `os` instead
//! of holding a list of its own.
//!
//! A name the host does not define is ABSENT rather than zero, inherited from
//! the same rule `os/constants.rs` states: `if (constants.SIGKILL)` must find
//! nothing on Windows, and a `0` there would read as "signal number zero".
//!
//! # Not implemented, by name
//!
//! - **`PRIORITY_LOW`, `PRIORITY_BELOW_NORMAL`, `PRIORITY_NORMAL`,
//!   `PRIORITY_ABOVE_NORMAL`, `PRIORITY_HIGH`, `PRIORITY_HIGHEST`** and
//!   **`UV_UDP_REUSEADDR`** — present in `os.constants`, absent HERE because
//!   Node does not spread `os.constants.priority` or `os.constants.libuv` into
//!   this module. Both post-date it. Adding them would make this crate's
//!   `node:constants` a superset of Node's, which is a compatibility bug in the
//!   direction that is hardest to notice.
//! - **The OpenSSL half of `crypto.constants`** — `SSL_OP_*`,
//!   `defaultCoreCipherList`, `defaultCipherList`, `POINT_CONVERSION_*`,
//!   `ENGINE_METHOD_*`. Absent for `crypto`'s reason, unchanged by the flatten:
//!   they name switches into a library this crate does not link.
//! - **On Windows specifically**: the `RTLD_*` family (there is no `dlopen(3)`,
//!   and Node's `os.constants.dlopen` is empty there too), the POSIX-only
//!   signals (`SIGHUP`, `SIGKILL`, `SIGCHLD`, the job-control set …), and
//!   `EDQUOT`/`EMULTIHOP`/`ESTALE`, which the CRT has no values for.

use rts_core::entry::{Context, make_number, make_object, put_member};

/// The namespace `node:constants` is: one flat object, no sub-objects.
///
/// Built with [`make_object`] rather than `make_namespace` because there is not
/// a single function in it — and not with `make_prototype`, which is for a
/// class and panics on a repeated name.
pub fn namespace(context: &mut Context) -> u64 {
    let object = make_object(context);

    // Spread order is Node's, and it is load-bearing only where two tables
    // share a key — none do today, so this is fidelity rather than a fix.
    // Winsock sits with `errno` because that is the group `os.constants` merges
    // it into.
    #[cfg(windows)]
    let integers: &[&[(&str, i32)]] = &[
        crate::os::constants::DLOPEN,
        crate::os::constants::ERRNO,
        crate::os::constants::WINSOCK,
        crate::os::constants::SIGNALS,
    ];
    #[cfg(not(windows))]
    let integers: &[&[(&str, i32)]] = &[
        crate::os::constants::DLOPEN,
        crate::os::constants::ERRNO,
        crate::os::constants::SIGNALS,
    ];
    for table in integers {
        for (name, value) in *table {
            let number = make_number(f64::from(*value));
            put_member(context, object, name, number);
        }
    }

    // `fs`'s table is `i64` because the open flags are bit sets a 32-bit signed
    // type would eventually be the wrong shape for; every value in it is far
    // inside the range a double represents exactly.
    for (name, value) in crate::fs::constants::ENTRIES {
        let number = make_number(*value as f64);
        put_member(context, object, name, number);
    }

    // `crypto`'s are already `f64` — two of them are negative sentinels rather
    // than counts, which is why that table is not an integer one.
    for (name, value) in crate::crypto::CONSTANTS {
        let number = make_number(*value);
        put_member(context, object, name, number);
    }

    object
}
