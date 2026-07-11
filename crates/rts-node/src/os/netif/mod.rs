//! node:os — `os.networkInterfaces()`: `Record<name, NetworkInterfaceInfo[]>`.
//!
//! Real NIC enumeration: POSIX `getifaddrs(3)`; Windows `GetAdaptersAddresses`.
//! Each address yields `{ address, netmask, family, mac, internal, cidr }`
//! (plus `scopeid` for IPv6). `cidr` is computed from the netmask, or `null`
//! when the netmask is malformed. Returns `{}` when enumeration fails — never a
//! fabricated interface.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::collections::BTreeMap;
use std::net::IpAddr;

use super::words::{array_word, bool_w, null_w, num_word, object, object_word, str_word};

/// One resolved interface address.
pub struct IfEntry {
    pub name: String,
    pub address: IpAddr,
    pub netmask: IpAddr,
    pub mac: String,
    pub internal: bool,
    /// IPv6 scope id (0 for non-link-local / IPv4).
    pub scopeid: u32,
}

/// `os.networkInterfaces()` — the keyed object of per-interface address arrays.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_NETWORK_INTERFACES() -> u64 {
    // Preserve first-seen order per interface; group by name.
    let mut groups: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for e in collect() {
        if !groups.contains_key(&e.name) {
            order.push(e.name.clone());
        }
        groups.entry(e.name.clone()).or_default().push(entry_word(&e));
    }
    let keys: Vec<&str> = order.iter().map(|s| s.as_str()).collect();
    let values: Vec<i64> = order
        .iter()
        .map(|name| array_word(groups.remove(name).unwrap_or_default()))
        .collect();
    object(&keys, &values)
}

/// Encode one address as its `NetworkInterfaceInfo` OBJECT element word.
fn entry_word(e: &IfEntry) -> i64 {
    let is_v6 = e.address.is_ipv6();
    let family = if is_v6 { "IPv6" } else { "IPv4" };
    let cidr = match prefix_len(&e.netmask) {
        Some(p) => str_word(&format!("{}/{}", e.address, p)),
        None => null_w(),
    };
    if is_v6 {
        object_word(
            &["address", "netmask", "family", "mac", "internal", "cidr", "scopeid"],
            &[
                str_word(&e.address.to_string()),
                str_word(&e.netmask.to_string()),
                str_word(family),
                str_word(&e.mac),
                bool_w(e.internal),
                cidr,
                num_word(e.scopeid as f64),
            ],
        )
    } else {
        object_word(
            &["address", "netmask", "family", "mac", "internal", "cidr"],
            &[
                str_word(&e.address.to_string()),
                str_word(&e.netmask.to_string()),
                str_word(family),
                str_word(&e.mac),
                bool_w(e.internal),
                cidr,
            ],
        )
    }
}

/// CIDR prefix length (count of leading 1-bits) of a netmask, or `None` if the
/// mask is not a contiguous run of ones (malformed → Node yields `cidr: null`).
fn prefix_len(mask: &IpAddr) -> Option<u32> {
    let count_contiguous = |bytes: &[u8]| -> Option<u32> {
        let mut bits = 0u32;
        let mut seen_zero = false;
        for &b in bytes {
            for i in (0..8).rev() {
                if (b >> i) & 1 == 1 {
                    if seen_zero {
                        return None; // 1 after a 0 → not a valid mask
                    }
                    bits += 1;
                } else {
                    seen_zero = true;
                }
            }
        }
        Some(bits)
    };
    match mask {
        IpAddr::V4(a) => count_contiguous(&a.octets()),
        IpAddr::V6(a) => count_contiguous(&a.octets()),
    }
}

fn collect() -> Vec<IfEntry> {
    #[cfg(unix)]
    {
        unix::collect()
    }
    #[cfg(windows)]
    {
        windows::collect()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Vec::new()
    }
}

/// Format 6 MAC bytes as `xx:xx:xx:xx:xx:xx`.
pub(crate) fn format_mac(bytes: &[u8]) -> String {
    if bytes.len() < 6 || bytes.iter().all(|&b| b == 0) {
        return "00:00:00:00:00:00".to_string();
    }
    bytes[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
