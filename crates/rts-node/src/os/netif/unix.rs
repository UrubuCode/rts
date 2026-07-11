//! POSIX `os.networkInterfaces()` via `getifaddrs(3)`. Two passes: build a
//! name→MAC map (Linux `AF_PACKET`/`sockaddr_ll`, macOS/BSD `AF_LINK`/
//! `sockaddr_dl`), then emit one entry per `AF_INET`/`AF_INET6` address.

use std::ffi::CStr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{format_mac, IfEntry};

pub fn collect() -> Vec<IfEntry> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates the list into `head`; freed below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 || head.is_null() {
        return Vec::new();
    }

    let macs = mac_map(head);
    let mut out = Vec::new();
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: cur is a valid list node until null.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() {
            continue;
        }
        let family = unsafe { (*ifa.ifa_addr).sa_family } as i32;
        let (address, scopeid) = match family {
            libc::AF_INET => (parse_v4(ifa.ifa_addr), 0),
            libc::AF_INET6 => parse_v6(ifa.ifa_addr),
            _ => continue,
        };
        let address = match address {
            Some(a) => a,
            None => continue,
        };
        let netmask = if ifa.ifa_netmask.is_null() {
            default_mask(&address)
        } else {
            match family {
                libc::AF_INET => parse_v4(ifa.ifa_netmask).unwrap_or(default_mask(&address)),
                libc::AF_INET6 => {
                    parse_v6(ifa.ifa_netmask).0.unwrap_or(default_mask(&address))
                }
                _ => default_mask(&address),
            }
        };
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
            .to_string_lossy()
            .into_owned();
        let internal = ifa.ifa_flags & (libc::IFF_LOOPBACK as u32) != 0;
        let mac = macs.get(&name).cloned().unwrap_or_else(|| format_mac(&[]));
        out.push(IfEntry { name, address, netmask, mac, internal, scopeid });
    }

    // SAFETY: matching free of the getifaddrs allocation.
    unsafe { libc::freeifaddrs(head) };
    out
}

/// name → MAC, from the link-layer list nodes.
fn mac_map(head: *mut libc::ifaddrs) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut cur = head;
    while !cur.is_null() {
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
            .to_string_lossy()
            .into_owned();
        if let Some(mac) = link_mac(ifa.ifa_addr) {
            map.insert(name, mac);
        }
    }
    map
}

#[cfg(target_os = "linux")]
fn link_mac(addr: *const libc::sockaddr) -> Option<String> {
    let family = unsafe { (*addr).sa_family } as i32;
    if family != libc::AF_PACKET {
        return None;
    }
    // SAFETY: AF_PACKET address is a sockaddr_ll.
    let sll = unsafe { &*(addr as *const libc::sockaddr_ll) };
    let len = sll.sll_halen as usize;
    if len == 0 {
        return None;
    }
    let bytes: Vec<u8> = sll.sll_addr.iter().take(len.min(6)).copied().collect();
    Some(format_mac(&bytes))
}

#[cfg(not(target_os = "linux"))]
fn link_mac(addr: *const libc::sockaddr) -> Option<String> {
    let family = unsafe { (*addr).sa_family } as i32;
    if family != libc::AF_LINK {
        return None;
    }
    // SAFETY: AF_LINK address is a sockaddr_dl; MAC bytes live at
    // sdl_data[sdl_nlen .. sdl_nlen + sdl_alen].
    let sdl = unsafe { &*(addr as *const libc::sockaddr_dl) };
    let alen = sdl.sdl_alen as usize;
    if alen == 0 {
        return None;
    }
    let start = sdl.sdl_nlen as usize;
    let data: Vec<u8> = sdl
        .sdl_data
        .iter()
        .skip(start)
        .take(alen.min(6))
        .map(|&c| c as u8)
        .collect();
    Some(format_mac(&data))
}

fn parse_v4(addr: *const libc::sockaddr) -> Option<IpAddr> {
    // SAFETY: caller guarantees an AF_INET sockaddr.
    let sin = unsafe { &*(addr as *const libc::sockaddr_in) };
    // s_addr holds the address in network byte order; its in-memory bytes are
    // the octets on either endianness.
    let octets = sin.sin_addr.s_addr.to_ne_bytes();
    Some(IpAddr::V4(Ipv4Addr::from(octets)))
}

fn parse_v6(addr: *const libc::sockaddr) -> (Option<IpAddr>, u32) {
    // SAFETY: caller guarantees an AF_INET6 sockaddr.
    let sin6 = unsafe { &*(addr as *const libc::sockaddr_in6) };
    let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
    (Some(IpAddr::V6(ip)), sin6.sin6_scope_id)
}

fn default_mask(addr: &IpAddr) -> IpAddr {
    match addr {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::from([0xffu8; 16])),
    }
}
