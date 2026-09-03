//! `socket.setMulticastInterface(interfaceAddress)` — `IP_MULTICAST_IF`,
//! IPv4-by-address only.
//!
//! # Reuse-check
//!
//! Same shape as [`super::bufsize`]'s own header: `std::net::UdpSocket` has no
//! accessor for this option, and the crate manifest already vets `libc` (Unix)
//! and a bare `extern "system"` Win32 declaration
//! (`crates/rts-node/src/os/*.rs`, `bufsize.rs`'s own `mod win`) for exactly
//! this shape of single `setsockopt`. `bufsize.rs`'s Windows constants are
//! `SOL_SOCKET`-level and `pub(super)` to that module, so they are not
//! reachable here — this module hand-declares its own binding rather than
//! widen that visibility for one caller.
//!
//! # Why the constant is verified rather than typed from memory
//!
//! `IP_MULTICAST_IF` is **32** on Linux and **9** on Windows — checked here
//! against `libc` (`unix/linux_like/mod.rs`) and `windows-sys`
//! (`Win32::Networking::WinSock`, cached locally though not a dependency of
//! this crate) rather than recalled. A wrong numeric optname is exactly the
//! silent-wrong-answer class CLAUDE.md's honesty floor ranks worst: it would
//! not fail to compile or to call, it would set a DIFFERENT option and still
//! answer success. `libc::IP_MULTICAST_IF` is used directly on Unix for the
//! same reason — the crate's own per-platform constant, not a copied number.
//!
//! # What this does NOT cover
//!
//! IPv6 (`setMulticastInterface` on a `udp6` socket, by interface index or
//! `%name`) — no fixture in this crate's suite exercises it, and `std` gives
//! no interface-name resolver; see `dgram/mod.rs`'s own "Not implemented"
//! list for the honest refusal that path takes instead of guessing.

use std::net::{Ipv4Addr, UdpSocket};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

/// Sets `IP_MULTICAST_IF` to `iface` — a 4-byte `in_addr` in network byte
/// order, the "select by address" payload both platforms document (`man 7
/// ip`'s "argument is an `in_addr` structure"), as opposed to the larger
/// `ip_mreqn`/index form neither test in this crate's suite asks for.
#[cfg(unix)]
pub(super) fn set_v4(socket: &UdpSocket, iface: Ipv4Addr) -> bool {
    let fd = socket.as_raw_fd();
    let value = iface.octets();
    // SAFETY: `value` is 4 bytes read by `setsockopt`, never written; `fd` is
    // a socket this module owns for the life of the call.
    let ok = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_IP,
            libc::IP_MULTICAST_IF,
            value.as_ptr().cast::<libc::c_void>(),
            value.len() as libc::socklen_t,
        )
    };
    ok == 0
}

#[cfg(windows)]
mod win {
    // Verified against `windows-sys` (`Win32::Networking::WinSock`), cached
    // locally though not a dependency — see this module's own doc for why the
    // number is checked rather than recalled.
    pub(super) const IPPROTO_IP: i32 = 0;
    pub(super) const IP_MULTICAST_IF: i32 = 9;

    unsafe extern "system" {
        pub(super) fn setsockopt(
            s: usize,
            level: i32,
            optname: i32,
            optval: *const i8,
            optlen: i32,
        ) -> i32;
    }
}

#[cfg(windows)]
pub(super) fn set_v4(socket: &UdpSocket, iface: Ipv4Addr) -> bool {
    let handle = socket.as_raw_socket() as usize;
    let value = iface.octets();
    // SAFETY: `value` is 4 bytes read by Winsock, never written; `handle` is a
    // socket this module owns for the life of the call.
    let ok = unsafe {
        win::setsockopt(
            handle,
            win::IPPROTO_IP,
            win::IP_MULTICAST_IF,
            value.as_ptr().cast::<i8>(),
            value.len() as i32,
        )
    };
    ok == 0
}

#[cfg(not(any(unix, windows)))]
pub(super) fn set_v4(_socket: &UdpSocket, _iface: Ipv4Addr) -> bool {
    false
}
