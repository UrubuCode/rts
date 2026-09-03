//! Multicast: `setMulticastTTL`, `setMulticastLoopback`,
//! `addMembership`/`dropMembership`, `setMulticastInterface`,
//! `add`/`dropSourceSpecificMembership`.

use rts_core::entry;

use super::common::{get_bool, with_socket};

/// `socket.setMulticastTTL(ttl)` — `udp4` only, see the module doc.
pub(super) extern "C" fn set_multicast_ttl(_e: u64, this: u64, ttl: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if get_bool(this, "__udp6") {
        return entry::undefined_value();
    }
    let ttl = entry::number_of(ttl).unwrap_or(1.0) as u32;
    with_socket(this, |socket| socket.set_multicast_ttl_v4(ttl));
    entry::undefined_value()
}

/// `socket.setMulticastLoopback(flag)`.
pub(super) extern "C" fn set_multicast_loopback(_e: u64, this: u64, flag: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let flag = entry::to_boolean(flag);
    let is_udp6 = get_bool(this, "__udp6");
    with_socket(this, |socket| if is_udp6 { socket.set_multicast_loop_v6(flag) } else { socket.set_multicast_loop_v4(flag) });
    entry::undefined_value()
}

/// `socket.addMembership(multicastAddress, multicastInterface?)` — the
/// interface argument is read only far enough to pick `INADDR_ANY`/`::` when
/// absent; a specific interface address IS honoured (`std` takes one), a
/// by-name/by-index interface is not (see the module doc).
pub(super) extern "C" fn add_membership(_e: u64, this: u64, group: u64, iface: u64, _c: u64, _d: u64) -> u64 {
    membership(this, group, iface, true);
    entry::undefined_value()
}

/// `socket.dropMembership(multicastAddress, multicastInterface?)`.
pub(super) extern "C" fn drop_membership(_e: u64, this: u64, group: u64, iface: u64, _c: u64, _d: u64) -> u64 {
    membership(this, group, iface, false);
    entry::undefined_value()
}

/// A `group` that fails to parse in the socket's own family — a malformed
/// address, or one from the OTHER family (`"ff02::1"` on a `udp4` socket) —
/// is `EINVAL`, raised BEFORE [`with_socket`] so the check runs outside its
/// lock, matching [`super::args`]'s own convention. Both `badGroupErr` and
/// `familyMismatchErr` in `tests/node_dgram_options.test.ts` land on this one
/// parse failure, which is why one raise covers both.
fn membership(this: u64, group: u64, iface: u64, join: bool) {
    let Some(group_text) = entry::text_of(group) else { return };
    let is_udp6 = get_bool(this, "__udp6");
    let syscall = if join { "addMembership" } else { "dropMembership" };
    if is_udp6 {
        let Ok(group) = group_text.parse::<std::net::Ipv6Addr>() else {
            crate::errors::system_error(syscall, "EINVAL");
            return;
        };
        with_socket(this, |socket| {
            if join { socket.join_multicast_v6(&group, 0) } else { socket.leave_multicast_v6(&group, 0) }
        });
    } else {
        let Ok(group) = group_text.parse::<std::net::Ipv4Addr>() else {
            crate::errors::system_error(syscall, "EINVAL");
            return;
        };
        let interface = entry::text_of(iface).and_then(|text| text.parse().ok()).unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        with_socket(this, |socket| {
            if join { socket.join_multicast_v4(&group, &interface) } else { socket.leave_multicast_v4(&group, &interface) }
        });
    }
}

/// `socket.setMulticastInterface(interfaceAddress)` — IPv4 by address, via
/// [`super::mcast_if`]; see that module's own doc for why the `udp6` form is
/// refused rather than guessed.
pub(super) extern "C" fn set_multicast_interface(_e: u64, this: u64, iface: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if get_bool(this, "__udp6") {
        crate::errors::system_error("setMulticastInterface", "ENOSYS");
        return absent;
    }
    let Some(text) = entry::with_runtime(|context| entry::string_in(context, iface)) else {
        crate::errors::system_error("setMulticastInterface", "EINVAL");
        return absent;
    };
    let Ok(address) = text.parse::<std::net::Ipv4Addr>() else {
        crate::errors::system_error("setMulticastInterface", "EINVAL");
        return absent;
    };
    with_socket(this, |socket| {
        if super::mcast_if::set_v4(socket, address) { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    });
    absent
}

/// `socket.addSourceSpecificMembership(source, group, interface?)` —
/// validation only; see [`super::ssm`]'s own doc for what happens past it.
pub(super) extern "C" fn add_source_specific_membership(_e: u64, this: u64, source: u64, group: u64, _c: u64, _d: u64) -> u64 {
    source_specific_membership("addSourceSpecificMembership", this, source, group);
    entry::undefined_value()
}

/// `socket.dropSourceSpecificMembership(source, group, interface?)`.
pub(super) extern "C" fn drop_source_specific_membership(_e: u64, this: u64, source: u64, group: u64, _c: u64, _d: u64) -> u64 {
    source_specific_membership("dropSourceSpecificMembership", this, source, group);
    entry::undefined_value()
}

fn source_specific_membership(syscall: &str, this: u64, source: u64, group: u64) {
    let is_udp6 = get_bool(this, "__udp6");
    if super::ssm::validate(syscall, is_udp6, source, group) {
        crate::errors::system_error(syscall, "ENOSYS");
    }
}
