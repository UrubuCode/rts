//! node:dgram — multicast: group membership (ASM), source-specific membership
//! (SSM, IGMPv3/MLDv2) and the outgoing-interface selection.
//!
//! IPv4 ASM/SSM and IPv6 ASM come from `socket2`. IPv6 SSM has no coverage in
//! `std` or `socket2`, so it is a raw `setsockopt(MCAST_JOIN_SOURCE_GROUP)` with
//! a hand-built `group_source_req` — see [`ssm6`].
//!
//! Interface arguments follow Node's platform rules (dgram.md §4): IPv4 takes the
//! interface's own IP; IPv6 takes a scope — an interface NAME on POSIX
//! (`'::%eth1'`), an interface NUMBER on Windows (`'::%2'`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use super::errors;
use super::lifecycle::{ensure_bound, open};
use super::state::SocketState;
use crate::values::read;

/// The socket a membership call applies to: bound (implicitly if needed) and open.
fn bound_socket(this: u64) -> Option<Arc<SocketState>> {
    let st = open(this)?;
    // Node auto-binds an unbound socket before joining a group.
    if let Err(e) = ensure_bound(this, &st) {
        errors::throw_io(&e, "bind");
        return None;
    }
    Some(st)
}

/// Parse a multicast group address for the socket's family.
fn group_of(st: &SocketState, s: &str, op: &str) -> Option<IpAddr> {
    match s.parse::<IpAddr>() {
        Ok(ip) if ip.is_ipv6() == st.v6 => Some(ip),
        _ => {
            errors::throw(
                "EINVAL",
                &format!("{op}: '{s}' is not a valid {} multicast address", family(st.v6)),
            );
            None
        }
    }
}

fn family(v6: bool) -> &'static str {
    if v6 {
        "IPv6"
    } else {
        "IPv4"
    }
}

/// The IPv4 interface argument: the interface's own address (`'0.0.0.0'` =
/// system default).
fn iface_v4(s: Option<&str>) -> Result<Ipv4Addr, String> {
    match s {
        None => Ok(Ipv4Addr::UNSPECIFIED),
        Some(a) => a
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("'{a}' is not a valid IPv4 interface address")),
    }
}

/// The IPv6 interface argument: an interface INDEX. Node accepts a scope —
/// a name on POSIX, a number on Windows — and `None`/`0` means "let the system
/// choose". A bare name is resolved through `if_nametoindex`, which exists under
/// that name on both platforms.
fn iface_v6(s: Option<&str>) -> Result<u32, String> {
    let Some(raw) = s else { return Ok(0) };
    let scope = raw.rsplit('%').next().unwrap_or(raw);
    if scope.is_empty() || raw == "::" {
        return Ok(0);
    }
    if let Ok(index) = scope.parse::<u32>() {
        return Ok(index);
    }
    match if_nametoindex(scope) {
        0 => Err(format!("'{raw}' does not name an interface")),
        index => Ok(index),
    }
}

/// `if_nametoindex(3)` / the IP Helper API's `if_nametoindex` — same name on
/// both platforms.
fn if_nametoindex(name: &str) -> u32 {
    let Ok(c) = std::ffi::CString::new(name) else {
        return 0;
    };
    #[cfg(unix)]
    unsafe {
        libc::if_nametoindex(c.as_ptr())
    }
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn if_nametoindex(name: *const std::ffi::c_char) -> u32;
        }
        unsafe { if_nametoindex(c.as_ptr()) }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = c;
        0
    }
}

/// `addMembership`/`dropMembership` share everything but the syscall.
fn membership(this: u64, gp: *const u8, gl: i64, iface: Option<String>, join: bool) {
    let op = if join { "addMembership" } else { "dropMembership" };
    let Some(st) = bound_socket(this) else { return };
    let group = read(gp, gl);
    let Some(group) = group_of(&st, &group, op) else { return };
    let result = match group {
        IpAddr::V4(g) => match iface_v4(iface.as_deref()) {
            Ok(i) if join => st.sock.join_multicast_v4(&g, &i),
            Ok(i) => st.sock.leave_multicast_v4(&g, &i),
            Err(msg) => return errors::throw("EINVAL", &format!("{op}: {msg}")),
        },
        IpAddr::V6(g) => match iface_v6(iface.as_deref()) {
            Ok(i) if join => st.sock.join_multicast_v6(&g, i),
            Ok(i) => st.sock.leave_multicast_v6(&g, i),
            Err(msg) => return errors::throw("EINVAL", &format!("{op}: {msg}")),
        },
    };
    if let Err(e) = result {
        errors::throw_io(&e, op);
    }
}

/// `socket.addMembership(multicastAddress)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP(this: u64, gp: *const u8, gl: i64) {
    membership(this, gp, gl, None, true);
}

/// `socket.addMembership(multicastAddress, multicastInterface)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP_IF(
    this: u64,
    gp: *const u8,
    gl: i64,
    ip: *const u8,
    il: i64,
) {
    membership(this, gp, gl, Some(read(ip, il)), true);
}

/// `socket.dropMembership(multicastAddress)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP(this: u64, gp: *const u8, gl: i64) {
    membership(this, gp, gl, None, false);
}

/// `socket.dropMembership(multicastAddress, multicastInterface)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP_IF(
    this: u64,
    gp: *const u8,
    gl: i64,
    ip: *const u8,
    il: i64,
) {
    membership(this, gp, gl, Some(read(ip, il)), false);
}

/// `add/dropSourceSpecificMembership(sourceAddress, groupAddress[, iface])`.
fn ssm(this: u64, source: String, group: String, iface: Option<String>, join: bool) {
    let op = if join {
        "addSourceSpecificMembership"
    } else {
        "dropSourceSpecificMembership"
    };
    let Some(st) = bound_socket(this) else { return };
    let Some(group) = group_of(&st, &group, op) else { return };
    let Ok(source) = source.parse::<IpAddr>() else {
        return errors::throw("EINVAL", &format!("{op}: '{source}' is not a valid source address"));
    };
    if source.is_ipv6() != group.is_ipv6() {
        return errors::throw("EINVAL", &format!("{op}: source and group must be the same family"));
    }
    let result = match (source, group) {
        (IpAddr::V4(s), IpAddr::V4(g)) => match iface_v4(iface.as_deref()) {
            Ok(i) if join => st.sock.join_ssm_v4(&s, &g, &i),
            Ok(i) => st.sock.leave_ssm_v4(&s, &g, &i),
            Err(msg) => return errors::throw("EINVAL", &format!("{op}: {msg}")),
        },
        (IpAddr::V6(s), IpAddr::V6(g)) => match iface_v6(iface.as_deref()) {
            Ok(i) => ssm6(&st, s, g, i, join),
            Err(msg) => return errors::throw("EINVAL", &format!("{op}: {msg}")),
        },
        _ => unreachable!("family mismatch is rejected above"),
    };
    if let Err(e) = result {
        errors::throw_io(&e, op);
    }
}

/// IPv6 source-specific multicast: `setsockopt(IPPROTO_IPV6,
/// MCAST_{JOIN,LEAVE}_SOURCE_GROUP, group_source_req)`. Neither `std` nor
/// `socket2` covers MLDv2 SSM, so the request struct is built by hand — it has
/// the same layout on every platform RTS targets: a `u32` interface index, then
/// the group and source as `sockaddr_storage` (with the padding the alignment of
/// `sockaddr_storage` forces after the index).
fn ssm6(st: &SocketState, source: Ipv6Addr, group: Ipv6Addr, index: u32, join: bool) -> std::io::Result<()> {
    use socket2::{SockAddr, SockAddrStorage};
    use std::net::SocketAddrV6;
    use std::os::raw::c_int;

    // `repr(C)` reproduces the C layout exactly, including the padding the
    // storage's 8-byte alignment forces after `interface` — the same padding the
    // C compiler inserts into `group_source_req`/`GROUP_SOURCE_REQ`.
    #[repr(C)]
    struct GroupSourceReq {
        interface: u32,
        group: SockAddrStorage,
        source: SockAddrStorage,
    }

    /// The address as a zero-padded `sockaddr_storage`, which is what the option
    /// carries (port/flowinfo are ignored for a membership request).
    fn storage_for(addr: Ipv6Addr) -> SockAddrStorage {
        SockAddr::from(SocketAddrV6::new(addr, 0, 0, 0)).as_storage()
    }

    let req = GroupSourceReq {
        interface: index,
        group: storage_for(group),
        source: storage_for(source),
    };
    let Some((join_opt, leave_opt)) = ssm6_options() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "IPv6 source-specific multicast is not supported on this platform",
        ));
    };
    let level = ipproto_ipv6();
    let name = if join { join_opt } else { leave_opt };
    let ptr = (&raw const req).cast::<u8>();
    let len = std::mem::size_of::<GroupSourceReq>() as c_int;
    // SAFETY: `ptr`/`len` describe the fully-initialized `req` for the duration
    // of the call, and the fd outlives it (borrowed from the live socket).
    let rc = unsafe { setsockopt_raw(st, level, name, ptr, len) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// `IPPROTO_IPV6` — 41 on every platform RTS targets (Windows, Linux, macOS).
fn ipproto_ipv6() -> std::os::raw::c_int {
    41
}

/// `(MCAST_JOIN_SOURCE_GROUP, MCAST_LEAVE_SOURCE_GROUP)` — RFC 3678's
/// protocol-independent SSM options. The numbers are ABI, and each family picked
/// its own: `ws2ipdef.h` (Windows), `<linux/in.h>` (Linux), `<netinet/in.h>`
/// (the BSD/Darwin lineage). `None` = the platform has no MLDv2 SSM option, and
/// the caller reports a real `ENOTSUP` rather than firing a wrong `setsockopt`.
fn ssm6_options() -> Option<(std::os::raw::c_int, std::os::raw::c_int)> {
    #[cfg(windows)]
    {
        Some((45, 46))
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Some((46, 47))
    }
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly"
    ))]
    {
        Some((82, 83))
    }
    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly"
    )))]
    {
        None
    }
}

/// The raw `setsockopt` for the option `socket2` does not expose.
///
/// # Safety
/// `value`/`len` must describe an initialized buffer of the option's type.
#[cfg(unix)]
unsafe fn setsockopt_raw(
    st: &SocketState,
    level: std::os::raw::c_int,
    name: std::os::raw::c_int,
    value: *const u8,
    len: std::os::raw::c_int,
) -> std::os::raw::c_int {
    use std::os::fd::AsRawFd;
    unsafe { libc::setsockopt(st.sock.as_raw_fd(), level, name, value.cast(), len as libc::socklen_t) }
}

#[cfg(windows)]
unsafe fn setsockopt_raw(
    st: &SocketState,
    level: std::os::raw::c_int,
    name: std::os::raw::c_int,
    value: *const u8,
    len: std::os::raw::c_int,
) -> std::os::raw::c_int {
    use std::os::windows::io::AsRawSocket;
    unsafe extern "system" {
        fn setsockopt(
            s: usize,
            level: std::os::raw::c_int,
            optname: std::os::raw::c_int,
            optval: *const std::os::raw::c_char,
            optlen: std::os::raw::c_int,
        ) -> std::os::raw::c_int;
    }
    unsafe { setsockopt(st.sock.as_raw_socket() as usize, level, name, value.cast(), len) }
}

#[cfg(not(any(unix, windows)))]
unsafe fn setsockopt_raw(
    _st: &SocketState,
    _level: std::os::raw::c_int,
    _name: std::os::raw::c_int,
    _value: *const u8,
    _len: std::os::raw::c_int,
) -> std::os::raw::c_int {
    -1
}

/// `socket.addSourceSpecificMembership(sourceAddress, groupAddress)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ADD_SSM(
    this: u64,
    sp: *const u8,
    sl: i64,
    gp: *const u8,
    gl: i64,
) {
    ssm(this, read(sp, sl), read(gp, gl), None, true);
}

/// `socket.addSourceSpecificMembership(sourceAddress, groupAddress, iface)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ADD_SSM_IF(
    this: u64,
    sp: *const u8,
    sl: i64,
    gp: *const u8,
    gl: i64,
    ip: *const u8,
    il: i64,
) {
    ssm(this, read(sp, sl), read(gp, gl), Some(read(ip, il)), true);
}

/// `socket.dropSourceSpecificMembership(sourceAddress, groupAddress)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_DROP_SSM(
    this: u64,
    sp: *const u8,
    sl: i64,
    gp: *const u8,
    gl: i64,
) {
    ssm(this, read(sp, sl), read(gp, gl), None, false);
}

/// `socket.dropSourceSpecificMembership(sourceAddress, groupAddress, iface)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_DROP_SSM_IF(
    this: u64,
    sp: *const u8,
    sl: i64,
    gp: *const u8,
    gl: i64,
    ip: *const u8,
    il: i64,
) {
    ssm(this, read(sp, sl), read(gp, gl), Some(read(ip, il)), false);
}

/// `socket.setMulticastInterface(multicastInterface)` — the interface outgoing
/// multicast leaves from. `'0.0.0.0'`/`'::'` restores the system default.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_MULTICAST_IF(this: u64, p: *const u8, l: i64) {
    let Some(st) = open(this) else { return };
    if !st.is_bound() {
        return errors::throw_unbound();
    }
    let arg = read(p, l);
    let result = if st.v6 {
        match iface_v6(Some(&arg)) {
            Ok(index) => st.sock.set_multicast_if_v6(index),
            // Node: most IPv6 scope errors fall back to the system default
            // instead of throwing.
            Err(_) => st.sock.set_multicast_if_v6(0),
        }
    } else {
        match iface_v4(Some(&arg)) {
            Ok(addr) => st.sock.set_multicast_if_v4(&addr),
            Err(msg) => return errors::throw("EINVAL", &format!("setMulticastInterface: {msg}")),
        }
    };
    if let Err(e) = result {
        errors::throw_io(&e, "setMulticastInterface");
    }
}
