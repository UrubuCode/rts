//! node:net — the `BlockList` RULE ENGINE: the rule kinds, their canonical
//! string form, and the address-matching semantics.
//!
//! Pure logic, no ABI, no handles — every entry point takes/returns plain Rust
//! values. It mirrors Node's own C++ (`src/node_sockaddr.cc`:
//! `SocketAddressBlockList`) exactly, including the parts that are easy to get
//! subtly wrong:
//!
//!  - rules match **across families**: an IPv4 rule matches an IPv4-MAPPED IPv6
//!    address (`::ffff:1.2.3.4`) and vice versa, because Node dispatches
//!    `is_match`/`compare`/`in_network` on the (rule, address) family PAIR;
//!  - a range compares IPv4 in host byte order and IPv6 by its 16 bytes, and a
//!    cross-family compare where the IPv6 side is NOT v4-mapped is `unordered`
//!    — which makes both `>= start` and `<= end` false, i.e. no match;
//!  - the rule strings are Node's own (`"Address: IPv4 1.2.3.4"`,
//!    `"Range: IPv4 a-b"`, `"Subnet: IPv4 10.0.0.0/8"`) — they are API, since
//!    `toJSON()` emits them and `fromJSON()` parses them back.

use std::cmp::Ordering;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// One rule in a `BlockList`, in insertion order.
#[derive(Clone, PartialEq, Eq)]
pub enum Rule {
    /// `addAddress(addr)` — one exact address.
    Address(IpAddr),
    /// `addRange(start, end)` — an inclusive range.
    Range(IpAddr, IpAddr),
    /// `addSubnet(net, prefix)` — a CIDR block.
    Subnet(IpAddr, u8),
}

/// `"IPv4"`/`"IPv6"` — how Node renders a family in a rule string.
pub fn family_name(ip: IpAddr) -> &'static str {
    match ip {
        IpAddr::V4(_) => "IPv4",
        IpAddr::V6(_) => "IPv6",
    }
}

/// The family a `type` argument names (`'ipv4'`/`'ipv6'`), or `None` if it is
/// neither — Node validates this argument strictly.
pub fn family_of_type(kind: &str) -> Option<bool> {
    match kind {
        "ipv4" => Some(false),
        "ipv6" => Some(true),
        _ => None,
    }
}

/// Parse `text` as an address of the requested family (`v6` = `'ipv6'`). The
/// family must match: Node parses under the declared `type` and rejects an
/// address of the other family.
pub fn parse_addr(text: &str, v6: bool) -> Option<IpAddr> {
    match (text.parse::<IpAddr>().ok()?, v6) {
        (ip @ IpAddr::V4(_), false) => Some(ip),
        (ip @ IpAddr::V6(_), true) => Some(ip),
        _ => None,
    }
}

/// The number of bits a prefix may have for `ip`'s family.
pub fn max_prefix(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    }
}

impl Rule {
    /// The rule's canonical string — Node's `Rule::ToString()`. This is API:
    /// `blockList.rules` and `toJSON()` expose it, and `fromJSON()` reads it.
    pub fn to_rule_string(&self) -> String {
        match self {
            Rule::Address(ip) => format!("Address: {} {ip}", family_name(*ip)),
            Rule::Range(start, end) => format!("Range: {} {start}-{end}", family_name(*start)),
            Rule::Subnet(net, prefix) => format!("Subnet: {} {net}/{prefix}", family_name(*net)),
        }
    }

    /// Parse a rule string back (the `fromJSON` half of the round trip).
    /// `None` for anything that is not one of the three shapes.
    pub fn parse(text: &str) -> Option<Rule> {
        let (kind, rest) = text.split_once(": ")?;
        let (family, body) = rest.split_once(' ')?;
        let v6 = match family {
            "IPv4" => false,
            "IPv6" => true,
            _ => return None,
        };
        match kind {
            "Address" => Some(Rule::Address(parse_addr(body, v6)?)),
            "Range" => {
                // An IPv6 rule body is `start-end` and IPv6 text has no '-', so
                // splitting on the FIRST '-' is unambiguous for both families.
                let (start, end) = body.split_once('-')?;
                Some(Rule::Range(parse_addr(start, v6)?, parse_addr(end, v6)?))
            }
            "Subnet" => {
                let (net, prefix) = body.rsplit_once('/')?;
                let net = parse_addr(net, v6)?;
                let prefix: u8 = prefix.parse().ok()?;
                if prefix > max_prefix(net) {
                    return None;
                }
                Some(Rule::Subnet(net, prefix))
            }
            _ => None,
        }
    }

    /// Whether `addr` is covered by this rule.
    pub fn matches(&self, addr: IpAddr) -> bool {
        match self {
            Rule::Address(rule) => is_match(*rule, addr),
            Rule::Range(start, end) => {
                matches!(compare(addr, *start), Some(Ordering::Greater | Ordering::Equal))
                    && matches!(compare(addr, *end), Some(Ordering::Less | Ordering::Equal))
            }
            Rule::Subnet(net, prefix) => in_network(addr, *net, *prefix),
        }
    }
}

/// The IPv4 address embedded in a v4-MAPPED IPv6 address (`::ffff:a.b.c.d`),
/// which is the only IPv6 shape Node's cross-family comparisons accept.
fn mapped_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    ip.to_ipv4_mapped()
}

/// Node's `SocketAddress::is_match` — exact equality, dispatched on the family
/// pair. A v4 and a v6 address match iff the v6 one is the v4-mapped form.
fn is_match(one: IpAddr, two: IpAddr) -> bool {
    match (one, two) {
        (IpAddr::V4(a), IpAddr::V4(b)) => a == b,
        (IpAddr::V6(a), IpAddr::V6(b)) => a == b,
        (IpAddr::V4(a), IpAddr::V6(b)) | (IpAddr::V6(b), IpAddr::V4(a)) => {
            mapped_v4(b) == Some(a)
        }
    }
}

/// Node's `compare` — the ordering a range rule uses. `None` is Node's
/// `partial_ordering::unordered` (a v4 against a non-mapped v6): both `>=` and
/// `<=` are then false, so such an address never falls inside the range.
fn compare(one: IpAddr, two: IpAddr) -> Option<Ordering> {
    match (one, two) {
        // Host byte order for v4, and the 16 bytes big-endian for v6 — u32/u128
        // from the octets reproduce exactly Node's ntohl/memcmp ordering.
        (IpAddr::V4(a), IpAddr::V4(b)) => Some(u32::from(a).cmp(&u32::from(b))),
        (IpAddr::V6(a), IpAddr::V6(b)) => Some(u128::from(a).cmp(&u128::from(b))),
        (IpAddr::V4(a), IpAddr::V6(b)) => Some(u32::from(a).cmp(&u32::from(mapped_v4(b)?))),
        (IpAddr::V6(a), IpAddr::V4(b)) => Some(u32::from(mapped_v4(a)?).cmp(&u32::from(b))),
    }
}

/// Node's `is_in_network` — CIDR containment across the four family pairs.
fn in_network(ip: IpAddr, net: IpAddr, prefix: u8) -> bool {
    match (ip, net) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => v4_in_v4(ip, net, prefix),
        (IpAddr::V6(ip), IpAddr::V6(net)) => v6_in_v6(ip, net, prefix),
        // A v4 address against a v6 network: Node maps the address into
        // `::ffff:a.b.c.d` and applies the v6 rule to it.
        (IpAddr::V4(ip), IpAddr::V6(net)) => v6_in_v6(ip.to_ipv6_mapped(), net, prefix),
        // A v6 address against a v4 network: only a v4-mapped address can be in
        // it, compared over the embedded v4 with the v4 prefix.
        (IpAddr::V6(ip), IpAddr::V4(net)) => match mapped_v4(ip) {
            Some(ip) => v4_in_v4(ip, net, prefix),
            None => false,
        },
    }
}

fn v4_in_v4(ip: Ipv4Addr, net: Ipv4Addr, prefix: u8) -> bool {
    // `prefix == 0` masks everything off (every address matches) and
    // `prefix == 32` needs all 32 bits — both fall out of the 64-bit shift, the
    // same way Node's `1ull << prefix` does.
    let mask = (((1u64 << prefix) - 1) << (32 - prefix)) as u32;
    (u32::from(ip) & mask) == (u32::from(net) & mask)
}

fn v6_in_v6(ip: Ipv6Addr, net: Ipv6Addr, prefix: u8) -> bool {
    if prefix == 128 {
        return ip == net;
    }
    let r = prefix % 8;
    let len = ((prefix - r) / 8) as usize;
    // `r == 0` (a byte-aligned prefix) means the partial-byte check is vacuous —
    // mask 0, exactly as Node's `((1 << 0) - 1) << 8` evaluates to 0.
    let mask: u8 = (((1u16 << r) - 1) << (8 - r).min(7)) as u8;
    let (ip, net) = (ip.octets(), net.octets());
    if ip[..len] != net[..len] {
        return false;
    }
    // `len < 16` always holds here: prefix == 128 returned above, so the largest
    // prefix reaching this point is 127 → len == 15.
    (ip[len] & mask) == (net[len] & mask)
}

/// A `SocketAddress`-shaped value: what `net.SocketAddress` holds and what
/// `SocketAddress.parse` produces.
pub struct AddressValue {
    pub address: IpAddr,
    pub port: u16,
    pub flowlabel: u32,
}

impl AddressValue {
    /// `SocketAddress.parse(input)` — `'1.2.3.4:80'` / `'[::1]:80'`. `None` on
    /// any parse failure (Node returns `undefined`, never throws).
    pub fn parse(input: &str) -> Option<AddressValue> {
        let (address, port) = if let Some(rest) = input.strip_prefix('[') {
            let (host, port) = rest.split_once("]:")?;
            (IpAddr::V6(host.parse::<Ipv6Addr>().ok()?), port)
        } else {
            let (host, port) = input.rsplit_once(':')?;
            (IpAddr::V4(host.parse::<Ipv4Addr>().ok()?), port)
        };
        Some(AddressValue {
            address,
            port: port.parse().ok()?,
            flowlabel: 0,
        })
    }
}

/// `net.isIP(input)` → 6 / 4 / 0.
pub fn ip_kind(text: &str) -> i32 {
    match text.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => 4,
        Ok(IpAddr::V6(_)) => 6,
        Err(_) => 0,
    }
}
