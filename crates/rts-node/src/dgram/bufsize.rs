//! `socket.get/setRecvBufferSize`, `socket.get/setSendBufferSize` —
//! `SO_RCVBUF`/`SO_SNDBUF` on the real OS socket.
//!
//! # Reuse-check
//!
//! `std::net::UdpSocket` has no accessor for either option (the module doc's
//! own refusal, written before this file existed). The old engine's
//! `crates/rts-node` answers this with the `socket2` crate, which is vetted
//! in `crates/rts-node/Cargo.toml` but **not** in
//! `docs/reference/node/crates.md`, the document this crate's manifest
//! actually follows — so it is not reused here without the owner adding it
//! there. What IS already vetted for `rts-node` is `libc` (§2 of that
//! doc: "libc-as-FFI-declaration is accepted... only a C compile/vendor step
//! is rejected"), and the raw-`extern "system"` FFI pattern
//! `crates/rts-node/src/os/*.rs` and `crates/rts-node/src/tty/
//! platform.rs` already use for Win32 calls with no crate behind them at
//! all. `setsockopt`/`getsockopt` on `SOL_SOCKET`/`SO_RCVBUF`/`SO_SNDBUF`
//! need nothing `socket2` has that a direct call does not: two numbers in,
//! one number out, on a file descriptor / `SOCKET` handle this module
//! already owns. So this hand-rolls the syscall on both platforms — `libc`
//! on Unix (already a dependency), a bare `ws2_32.dll` declaration on
//! Windows (same shape as `crates/rts-node/src/os/cpus.rs`'s existing
//! Win32 calls) — rather than adding `socket2`.

use std::net::UdpSocket;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

/// `SO_RCVBUF`/`SO_SNDBUF` share one implementation; `recv` selects which.
#[derive(Clone, Copy)]
pub(super) enum Which {
    Recv,
    Send,
}

#[cfg(unix)]
fn optname(which: Which) -> libc::c_int {
    match which {
        Which::Recv => libc::SO_RCVBUF,
        Which::Send => libc::SO_SNDBUF,
    }
}

#[cfg(unix)]
pub(super) fn get(socket: &UdpSocket, which: Which) -> Option<i32> {
    let fd = socket.as_raw_fd();
    let mut value: libc::c_int = 0;
    let mut len: libc::socklen_t = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `value`/`len` are valid for the write `getsockopt` performs, and
    // `fd` is a socket this module owns for the life of the call.
    let ok = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            optname(which),
            &mut value as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    (ok == 0).then_some(value)
}

#[cfg(unix)]
pub(super) fn set(socket: &UdpSocket, which: Which, size: i32) -> bool {
    let fd = socket.as_raw_fd();
    let value: libc::c_int = size;
    // SAFETY: `value` is read, not written; `fd` is a socket this module owns.
    let ok = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            optname(which),
            &value as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    ok == 0
}

#[cfg(windows)]
mod win {
    // Winsock defines these with the same numeric values BSD does; declared
    // here rather than pulled from a crate, matching `os/cpus.rs`'s own
    // `extern "system"` convention for a Win32 call this crate has no
    // dependency wrapping.
    pub(super) const SOL_SOCKET: i32 = 0xffff;
    pub(super) const SO_RCVBUF: i32 = 0x1002;
    pub(super) const SO_SNDBUF: i32 = 0x1001;

    unsafe extern "system" {
        pub(super) fn setsockopt(
            s: usize,
            level: i32,
            optname: i32,
            optval: *const i8,
            optlen: i32,
        ) -> i32;
        pub(super) fn getsockopt(
            s: usize,
            level: i32,
            optname: i32,
            optval: *mut i8,
            optlen: *mut i32,
        ) -> i32;
    }
}

#[cfg(windows)]
fn win_optname(which: Which) -> i32 {
    match which {
        Which::Recv => win::SO_RCVBUF,
        Which::Send => win::SO_SNDBUF,
    }
}

#[cfg(windows)]
pub(super) fn get(socket: &UdpSocket, which: Which) -> Option<i32> {
    let handle = socket.as_raw_socket() as usize;
    let mut value: i32 = 0;
    let mut len: i32 = std::mem::size_of::<i32>() as i32;
    // SAFETY: `value`/`len` are valid for the write Winsock performs, and
    // `handle` is a socket this module owns for the life of the call.
    let ok = unsafe {
        win::getsockopt(handle, win::SOL_SOCKET, win_optname(which), &mut value as *mut i32 as *mut i8, &mut len)
    };
    (ok == 0).then_some(value)
}

#[cfg(windows)]
pub(super) fn set(socket: &UdpSocket, which: Which, size: i32) -> bool {
    let handle = socket.as_raw_socket() as usize;
    let value: i32 = size;
    // SAFETY: `value` is read, not written; `handle` is a socket this module owns.
    let ok = unsafe {
        win::setsockopt(handle, win::SOL_SOCKET, win_optname(which), &value as *const i32 as *const i8, std::mem::size_of::<i32>() as i32)
    };
    ok == 0
}

#[cfg(not(any(unix, windows)))]
pub(super) fn get(_socket: &UdpSocket, _which: Which) -> Option<i32> {
    None
}

#[cfg(not(any(unix, windows)))]
pub(super) fn set(_socket: &UdpSocket, _which: Which, _size: i32) -> bool {
    false
}
