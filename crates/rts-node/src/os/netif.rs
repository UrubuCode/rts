//! `os.networkInterfaces()` — every address the host will admit to, grouped by
//! interface name.
//!
//! # Reuse check
//!
//! `net/ip.rs` in this crate parses and classifies an IP that arrived as TEXT;
//! nothing here can use it, because what arrives here is a `sockaddr` and the
//! question is how to READ one. The formatting is `std::net::Ipv4Addr`/
//! `Ipv6Addr`'s `Display`, which is the one place in the process that decides
//! how an address is spelled — hand-formatting the bytes would be a second
//! spelling of an IPv6 address, and the two would disagree about `::` folding.

use std::net::{Ipv4Addr, Ipv6Addr};

/// One address on one interface — Node's `NetworkInterfaceInfo`.
pub(super) struct Interface {
    pub(super) address: String,
    pub(super) netmask: String,
    pub(super) family: &'static str,
    pub(super) mac: String,
    pub(super) internal: bool,
    /// `None` becomes `null`, which is what Node answers when the prefix cannot
    /// be turned into CIDR notation.
    pub(super) cidr: Option<String>,
    /// `Some` only for IPv6, which is the only family the field exists on.
    pub(super) scopeid: Option<u32>,
}

/// Interfaces by name, in the OS's own order (Node does not sort either).
///
/// Empty when enumeration fails — Node's documented failure answer, and the
/// only case where an empty result here is not a claim that the host has no
/// addresses.
#[cfg(windows)]
pub(super) fn interfaces() -> Vec<(String, Vec<Interface>)> {
    // Skip the three address lists this never reads: anycast, multicast and DNS
    // servers are pointer chains the kernel would otherwise build for nothing.
    const SKIP_UNWANTED: u32 = 0x0002 | 0x0004 | 0x0008;
    const AF_UNSPEC: u32 = 0;
    const ERROR_BUFFER_OVERFLOW: u32 = 111;
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

    let mut size = 16 * 1024u32;
    // `u64` elements rather than bytes so the allocation is 8-aligned, which
    // `IpAdapterAddresses` requires and a `Vec<u8>` does not promise.
    let mut buffer = vec![0u64; size as usize / 8 + 1];
    let mut status = unsafe {
        GetAdaptersAddresses(AF_UNSPEC, SKIP_UNWANTED, std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size)
    };
    if status == ERROR_BUFFER_OVERFLOW {
        buffer = vec![0u64; size as usize / 8 + 1];
        status = unsafe {
            GetAdaptersAddresses(AF_UNSPEC, SKIP_UNWANTED, std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size)
        };
    }
    if status != 0 {
        return Vec::new();
    }

    let mut held: Vec<(String, Vec<Interface>)> = Vec::new();
    let mut adapter: *const IpAdapterAddresses = buffer.as_ptr().cast();
    while !adapter.is_null() {
        // Every dereference below is of a node the kernel wrote into `buffer`,
        // which outlives this loop, and of a `Next` chain it terminates with
        // null — the two invariants that make the walk sound.
        let current = unsafe { &*adapter };
        let name = wide_text(current.friendly_name);
        let mac = mac_text(&current.physical_address[..], current.physical_address_length as usize);
        let internal = current.if_type == IF_TYPE_SOFTWARE_LOOPBACK;
        let mut addresses = Vec::new();
        let mut unicast = current.first_unicast;
        while !unicast.is_null() {
            let entry = unsafe { &*unicast };
            let family = match entry.address.sockaddr.is_null() {
                true => 0,
                // On Windows every `sockaddr` starts with a 16-bit family, so
                // reading it here is sound; the BSD layout that starts with a
                // length byte is why the POSIX caller reads its own instead.
                false => i32::from(unsafe { std::ptr::read_unaligned(entry.address.sockaddr.cast::<u16>()) }),
            };
            if let Some(found) = from_sockaddr(entry.address.sockaddr, family, entry.on_link_prefix_length, &mac, internal) {
                addresses.push(found);
            }
            unicast = entry.next;
        }
        if !addresses.is_empty() {
            held.push((name, addresses));
        }
        adapter = current.next;
    }
    held
}

/// Interfaces by name, from `getifaddrs(3)`.
#[cfg(not(windows))]
pub(super) fn interfaces() -> Vec<(String, Vec<Interface>)> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Vec::new();
    }
    let mut held: Vec<(String, Vec<Interface>)> = Vec::new();
    let mut node = list;
    while !node.is_null() {
        let current = unsafe { &*node };
        let name = unsafe { std::ffi::CStr::from_ptr(current.ifa_name) }.to_string_lossy().into_owned();
        let internal = current.ifa_flags & libc::IFF_LOOPBACK as u32 != 0;
        let mac = mac_of(&name);
        if let Some(found) = from_sockaddrs(current.ifa_addr, current.ifa_netmask, &mac, internal) {
            match held.iter_mut().find(|(held_name, _)| *held_name == name) {
                Some((_, addresses)) => addresses.push(found),
                None => held.push((name, vec![found])),
            }
        }
        node = current.ifa_next;
    }
    unsafe { libc::freeifaddrs(list) };
    held
}

/// The object `os.networkInterfaces()` answers — interface name to array.
///
/// Built in one borrow, from a walk that already finished: see `cpus::value`
/// for why the two halves are split that way.
pub(super) fn value(
    context: &mut rts_core::entry::Context,
    held: Vec<(String, Vec<Interface>)>,
) -> u64 {
    let root = rts_core::entry::make_object(context);
    for (name, addresses) in held {
        let entries = addresses
            .into_iter()
            .map(|entry| {
                let object = rts_core::entry::make_object(context);
                for (key, text) in [
                    ("address", entry.address),
                    ("netmask", entry.netmask),
                    ("family", entry.family.to_owned()),
                    ("mac", entry.mac),
                ] {
                    let value = rts_core::entry::make_string(context, &text);
                    rts_core::entry::put_member(context, object, key, value);
                }
                let internal = rts_core::entry::boolean_value(entry.internal);
                rts_core::entry::put_member(context, object, "internal", internal);
                // `null`, not `undefined`, and not the absence of the key: Node
                // documents `cidr` as `string | null`, and a program testing
                // `=== null` would fail on either substitute.
                let cidr = match &entry.cidr {
                    Some(text) => rts_core::entry::make_string(context, text),
                    None => rts_core::entry::null_in(context),
                };
                rts_core::entry::put_member(context, object, "cidr", cidr);
                if let Some(scope) = entry.scopeid {
                    let value = rts_core::entry::make_number(f64::from(scope));
                    rts_core::entry::put_member(context, object, "scopeid", value);
                }
                object
            })
            .collect();
        let array = rts_core::entry::make_array_in(context, entries);
        rts_core::entry::put_member(context, root, &name, array);
    }
    root
}

/// The hardware address of a named interface.
///
/// Linux publishes it as a text file, so this is an ordinary `std::fs` read
/// rather than the `AF_PACKET`/`AF_LINK` pseudo-entry walk `getifaddrs` would
/// otherwise need. On every other Unix it is not read at all and the answer is
/// the all-zero placeholder — refused by name in `os/mod.rs` rather than
/// silently wrong.
#[cfg(not(windows))]
fn mac_of(name: &str) -> String {
    #[cfg(target_os = "linux")]
    if let Ok(text) = std::fs::read_to_string(format!("/sys/class/net/{name}/address")) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    let _ = name;
    "00:00:00:00:00:00".to_owned()
}

/// One address and its netmask, both as `sockaddr` pointers.
#[cfg(not(windows))]
fn from_sockaddrs(
    address: *mut libc::sockaddr,
    netmask: *mut libc::sockaddr,
    mac: &str,
    internal: bool,
) -> Option<Interface> {
    if address.is_null() {
        return None;
    }
    let family = i32::from(unsafe { (*address).sa_family });
    let prefix = prefix_of(netmask, family);
    from_sockaddr(address.cast(), family, prefix, mac, internal)
}

/// How many leading one-bits a netmask `sockaddr` carries.
///
/// Counted over the bytes rather than read from a prefix field, because
/// `getifaddrs` reports a MASK and Node reports both — the CIDR suffix is this
/// count and the netmask string is the mask rebuilt from it, so a mask with a
/// hole in it (which is malformed, and which Node answers `cidr: null` for)
/// stops the count at the hole rather than being silently rounded up.
#[cfg(not(windows))]
fn prefix_of(netmask: *mut libc::sockaddr, family: i32) -> u8 {
    if netmask.is_null() {
        return 0;
    }
    let bytes: [u8; 16] = if family == libc::AF_INET {
        let mask = unsafe { &*(netmask as *const libc::sockaddr_in) };
        let mut held = [0u8; 16];
        held[..4].copy_from_slice(&mask.sin_addr.s_addr.to_ne_bytes());
        held
    } else if family == libc::AF_INET6 {
        unsafe { &*(netmask as *const libc::sockaddr_in6) }.sin6_addr.s6_addr
    } else {
        return 0;
    };
    let mut bits = 0u8;
    for byte in bytes {
        bits += byte.leading_ones() as u8;
        if byte != 0xFF {
            break;
        }
    }
    bits
}

/// One `sockaddr` whose family the caller has already read.
///
/// The family comes IN rather than being read here because where it sits
/// differs: Windows and Linux start a `sockaddr` with a 16-bit family, and the
/// BSDs start it with a length byte. The rest — the address bytes at 4 (IPv4)
/// or 8 (IPv6) and the scope at 24 — is identical everywhere, which is what
/// makes one reader correct for all of them.
fn from_sockaddr(address: *const u8, family: i32, prefix: u8, mac: &str, internal: bool) -> Option<Interface> {
    if address.is_null() {
        return None;
    }
    if family == AF_INET {
        let octets: [u8; 4] = unsafe { std::ptr::read_unaligned(address.add(4).cast()) };
        let held = Ipv4Addr::from(octets);
        let prefix = prefix.min(32);
        let mask = match prefix {
            0 => 0u32,
            bits => u32::MAX << (32 - u32::from(bits)),
        };
        return Some(Interface {
            address: held.to_string(),
            netmask: Ipv4Addr::from(mask).to_string(),
            family: "IPv4",
            mac: mac.to_owned(),
            internal,
            cidr: Some(format!("{held}/{prefix}")),
            scopeid: None,
        });
    }
    if family == AF_INET6 {
        let octets: [u8; 16] = unsafe { std::ptr::read_unaligned(address.add(8).cast()) };
        let scope: u32 = unsafe { std::ptr::read_unaligned(address.add(24).cast()) };
        let held = Ipv6Addr::from(octets);
        let prefix = prefix.min(128);
        let mut mask = [0u8; 16];
        for (index, byte) in mask.iter_mut().enumerate() {
            let taken = usize::from(prefix).saturating_sub(index * 8).min(8);
            *byte = (0xFFu16 << (8 - taken)) as u8;
        }
        return Some(Interface {
            address: held.to_string(),
            netmask: Ipv6Addr::from(mask).to_string(),
            family: "IPv6",
            mac: mac.to_owned(),
            internal,
            cidr: Some(format!("{held}/{prefix}")),
            scopeid: Some(scope),
        });
    }
    None
}

/// `AF_INET`, which is `2` on every platform RTS targets — from `libc` where
/// there is one, so the number is the host's rather than this file's.
#[cfg(windows)]
const AF_INET: i32 = 2;
/// `AF_INET6` is `23` on Windows, `10` on Linux and `30` on the BSDs, which is
/// exactly why it is not written as a literal off Windows.
#[cfg(windows)]
const AF_INET6: i32 = 23;
#[cfg(not(windows))]
const AF_INET: i32 = libc::AF_INET;
#[cfg(not(windows))]
const AF_INET6: i32 = libc::AF_INET6;

/// A MAC address in Node's spelling; the all-zero placeholder when the adapter
/// has none, which Node also uses for loopback.
///
/// Windows only, because its one caller is: off Windows the address arrives
/// already formatted from `getifaddrs`. Without the `cfg` this is dead code on
/// every other platform, and this crate `#![deny(dead_code)]`s — so the Linux
/// leg of CI failed to COMPILE while Windows was green, which is the shape of
/// breakage a local build cannot show.
#[cfg(windows)]
fn mac_text(bytes: &[u8], length: usize) -> String {
    let taken = length.min(bytes.len());
    match taken {
        0 => "00:00:00:00:00:00".to_owned(),
        _ => bytes[..taken].iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":"),
    }
}

/// A NUL-terminated wide string the kernel wrote.
#[cfg(windows)]
fn wide_text(start: *const u16) -> String {
    if start.is_null() {
        return String::new();
    }
    let mut length = 0;
    while unsafe { *start.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(start, length) })
}

#[cfg(windows)]
#[repr(C)]
struct SocketAddress {
    sockaddr: *const u8,
    length: i32,
}

/// Truncated ON PURPOSE at `oper_status`: the real struct continues with a
/// dozen fields no caller here reads, and a shorter declaration is safe because
/// the walk follows the kernel's own `Next` pointers rather than indexing an
/// array of these.
#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct IpAdapterAddresses {
    length: u32,
    if_index: u32,
    next: *const IpAdapterAddresses,
    adapter_name: *const u8,
    first_unicast: *const IpAdapterUnicastAddress,
    first_anycast: *const u8,
    first_multicast: *const u8,
    first_dns_server: *const u8,
    dns_suffix: *const u16,
    description: *const u16,
    friendly_name: *const u16,
    physical_address: [u8; 8],
    physical_address_length: u32,
    flags: u32,
    mtu: u32,
    if_type: u32,
    oper_status: i32,
}

#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct IpAdapterUnicastAddress {
    length: u32,
    flags: u32,
    next: *const IpAdapterUnicastAddress,
    address: SocketAddress,
    prefix_origin: i32,
    suffix_origin: i32,
    dad_state: i32,
    valid_lifetime: u32,
    preferred_lifetime: u32,
    lease_lifetime: u32,
    on_link_prefix_length: u8,
}

#[cfg(windows)]
#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetAdaptersAddresses(
        family: u32,
        flags: u32,
        reserved: *mut u8,
        addresses: *mut IpAdapterAddresses,
        size: *mut u32,
    ) -> u32;
}
