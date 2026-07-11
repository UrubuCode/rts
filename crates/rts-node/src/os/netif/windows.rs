//! Windows `os.networkInterfaces()` via `GetAdaptersAddresses` (iphlpapi):
//! FriendlyName key, per-unicast address + `OnLinkPrefixLength`→netmask,
//! PhysicalAddress→MAC, `IfType == SOFTWARE_LOOPBACK`→`internal`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{format_mac, IfEntry};

// Windows address families (note AF_INET6 == 23 on Windows, not 10).
const AF_UNSPEC: u32 = 0;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
// GAA_FLAG_SKIP_ANYCAST | SKIP_MULTICAST | SKIP_DNS_SERVER.
const GAA_FLAGS: u32 = 0x0002 | 0x0004 | 0x0008;
const ERROR_BUFFER_OVERFLOW: u32 = 111;
const ERROR_SUCCESS: u32 = 0;

#[repr(C)]
struct SocketAddress {
    lp_sockaddr: *mut Sockaddr,
    i_sockaddr_length: i32,
}

#[repr(C)]
struct Sockaddr {
    sa_family: u16,
    _sa_data: [u8; 14],
}

#[repr(C)]
struct SockaddrIn {
    family: u16,
    _port: u16,
    addr: [u8; 4],
    _zero: [u8; 8],
}

#[repr(C)]
struct SockaddrIn6 {
    family: u16,
    _port: u16,
    _flowinfo: u32,
    addr: [u8; 16],
    scope_id: u32,
}

#[repr(C)]
struct IpAdapterUnicastAddress {
    _length: u32,
    _flags: u32,
    next: *mut IpAdapterUnicastAddress,
    address: SocketAddress,
    _prefix_origin: u32,
    _suffix_origin: u32,
    _dad_state: u32,
    _valid_lifetime: u32,
    _preferred_lifetime: u32,
    _lease_lifetime: u32,
    on_link_prefix_length: u8,
}

#[repr(C)]
struct IpAdapterAddresses {
    _length: u32,
    _if_index: u32,
    next: *mut IpAdapterAddresses,
    _adapter_name: *mut u8,
    first_unicast: *mut IpAdapterUnicastAddress,
    _first_anycast: *mut core::ffi::c_void,
    _first_multicast: *mut core::ffi::c_void,
    _first_dns_server: *mut core::ffi::c_void,
    _dns_suffix: *mut u16,
    _description: *mut u16,
    friendly_name: *mut u16,
    physical_address: [u8; 8],
    physical_address_length: u32,
    _flags: u32,
    _mtu: u32,
    if_type: u32,
    // Trailing LH fields omitted — never read past `if_type`.
}

#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetAdaptersAddresses(
        family: u32,
        flags: u32,
        reserved: *mut core::ffi::c_void,
        addresses: *mut IpAdapterAddresses,
        size: *mut u32,
    ) -> u32;
}

pub fn collect() -> Vec<IfEntry> {
    let mut size: u32 = 15 * 1024;
    let mut buf: Vec<u8> = Vec::new();
    // Two-call sizing, growing on ERROR_BUFFER_OVERFLOW.
    for _ in 0..4 {
        buf.resize(size as usize, 0);
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC,
                GAA_FLAGS,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut IpAdapterAddresses,
                &mut size,
            )
        };
        if rc == ERROR_SUCCESS {
            return walk(buf.as_ptr() as *const IpAdapterAddresses);
        }
        if rc != ERROR_BUFFER_OVERFLOW {
            return Vec::new();
        }
    }
    Vec::new()
}

fn walk(head: *const IpAdapterAddresses) -> Vec<IfEntry> {
    let mut out = Vec::new();
    let mut ada = head;
    while !ada.is_null() {
        // SAFETY: valid list node until null.
        let a = unsafe { &*ada };
        ada = a.next;
        let name = wide_to_string(a.friendly_name);
        let mac = format_mac(&a.physical_address[..a.physical_address_length.min(6) as usize]);
        let internal = a.if_type == IF_TYPE_SOFTWARE_LOOPBACK;

        let mut ua = a.first_unicast;
        while !ua.is_null() {
            // SAFETY: valid unicast node until null.
            let u = unsafe { &*ua };
            ua = u.next;
            if let Some((address, netmask, scopeid)) =
                parse_unicast(u.address.lp_sockaddr, u.on_link_prefix_length)
            {
                out.push(IfEntry {
                    name: name.clone(),
                    address,
                    netmask,
                    mac: mac.clone(),
                    internal,
                    scopeid,
                });
            }
        }
    }
    out
}

fn parse_unicast(sa: *const Sockaddr, prefix: u8) -> Option<(IpAddr, IpAddr, u32)> {
    if sa.is_null() {
        return None;
    }
    let family = unsafe { (*sa).sa_family };
    match family {
        AF_INET => {
            let sin = unsafe { &*(sa as *const SockaddrIn) };
            let ip = IpAddr::V4(Ipv4Addr::from(sin.addr));
            let mask = IpAddr::V4(ipv4_mask(prefix));
            Some((ip, mask, 0))
        }
        AF_INET6 => {
            let sin6 = unsafe { &*(sa as *const SockaddrIn6) };
            let ip = IpAddr::V6(Ipv6Addr::from(sin6.addr));
            let mask = IpAddr::V6(ipv6_mask(prefix));
            Some((ip, mask, sin6.scope_id))
        }
        _ => None,
    }
}

fn ipv4_mask(prefix: u8) -> Ipv4Addr {
    let p = prefix.min(32);
    let bits: u32 = if p == 0 { 0 } else { u32::MAX << (32 - p) };
    Ipv4Addr::from(bits)
}

fn ipv6_mask(prefix: u8) -> Ipv6Addr {
    let p = prefix.min(128) as usize;
    let mut octets = [0u8; 16];
    let mut remaining = p;
    for byte in octets.iter_mut() {
        if remaining >= 8 {
            *byte = 0xff;
            remaining -= 8;
        } else if remaining > 0 {
            *byte = 0xffu8 << (8 - remaining);
            remaining = 0;
        }
    }
    Ipv6Addr::from(octets)
}

fn wide_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    // SAFETY: NUL-terminated PWSTR.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}
