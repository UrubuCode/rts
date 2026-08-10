//! `net.BlockList` — not an `EventEmitter`, no background thread, so unlike
//! every other class in this module it needs no entry in `registry.rs` at
//! all: the rule set lives entirely as `this.rules`, a plain JS array of
//! strings in the exact format `toJSON()`/`fromJSON()` exchange (verified
//! against Node's `Rule::ToString()`, `src/node_sockaddr.cc` — see the
//! reference doc's own correction note). `check()` re-parses that array each
//! call rather than keeping a second, native copy: two representations of
//! one rule set is the drift this crate's own `docs/README.md` rule (via
//! `reuse-check`) exists to catch, and a linear scan over a handful of
//! strings costs nothing a native table would meaningfully save.

use rts_core::entry::{self, Provided};
use std::net::IpAddr;

const METHODS: &[(&str, Provided)] = &[
    ("addAddress", add_address),
    ("addRange", add_range),
    ("addSubnet", add_subnet),
    ("check", check),
    ("toJSON", to_json),
    ("fromJSON", from_json),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    entry::make_prototype(context, "BlockList", METHODS)
}

pub(super) extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::common::self_or_new(context, this, prototype);
        super::common::set_array(context, instance, "rules", Vec::new());
        instance
    })
}

/// `BlockList.isBlockList(value)` — an own `rules` array is this crate's
/// stand-in for the class check real Node does with a native wrapper type;
/// good enough for the values this module itself ever produces.
pub(super) extern "C" fn is_block_list(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let rules = entry::get_indexed(value, super::common::key("rules"));
    entry::boolean_value(rules != absent)
}

fn push_rule(this: u64, rule: String) {
    let rules = super::common::get_value(this, "rules");
    let absent = entry::undefined_value();
    let mut values: Vec<u64> = if rules == absent { Vec::new() } else { collect(rules) };
    let held = entry::with_runtime(|context| entry::make_string(context, &rule));
    values.push(held);
    entry::with_runtime(|context| super::common::set_array(context, this, "rules", values));
}

fn collect(array: u64) -> Vec<u64> {
    let length_value = entry::get_indexed(array, super::common::key("length"));
    let length = entry::number_of(length_value).map(|v| v as usize).unwrap_or(0);
    (0..length).map(|index| entry::get_indexed(array, entry::make_number(index as f64))).collect()
}

fn type_arg(value: u64) -> &'static str {
    match entry::text_of(value).as_deref() {
        Some("ipv6") => "IPv6",
        _ => "IPv4",
    }
}

/// An argument as an address string — a plain string, OR a `SocketAddress`
/// instance read off its own `address` property, matching Node's own
/// `string | SocketAddress` union for every `BlockList` add method. This used
/// to be `entry::text_of` alone, which does not fall through to a property
/// read, so a `SocketAddress` argument answered `None` where Node reads its
/// `.address`.
fn address_text(value: u64) -> Option<String> {
    if let Some(text) = entry::text_of(value) {
        return Some(text);
    }
    let absent = entry::undefined_value();
    let field = entry::get_indexed(value, super::common::key("address"));
    if field == absent {
        return None;
    }
    entry::text_of(field)
}

/// `ERR_INVALID_ADDRESS` — real Node raises a plain `Error` (not a
/// `TypeError`) with this code; `entry::throw_type_error` is the only raise
/// this crate can reach publicly (rule 8's own exemption list does not cover
/// a second error class), so the class here diverges from Node's while the
/// code/message text — what every test in this file actually checks — does
/// not.
fn invalid_address() {
    entry::throw_type_error("ERR_INVALID_ADDRESS: Invalid socket address");
}

extern "C" fn add_address(_e: u64, this: u64, address: u64, kind: u64, _c: u64, _d: u64) -> u64 {
    let Some(address) = address_text(address) else {
        invalid_address();
        return entry::undefined_value();
    };
    let family = type_arg(kind);
    let is_v6 = address.parse::<std::net::Ipv6Addr>().is_ok();
    let is_v4 = address.parse::<std::net::Ipv4Addr>().is_ok();
    if (family == "IPv4" && !is_v4) || (family == "IPv6" && !is_v6) {
        invalid_address();
        return entry::undefined_value();
    }
    push_rule(this, format!("Address: {family} {address}"));
    entry::undefined_value()
}

/// `blockList.addRange(start, end, type?)`. Four call slots total once the
/// receiver takes one — `type` reads from the last, matching the ceiling
/// this task's brief states.
///
/// `ERR_INVALID_ARG_VALUE` when `start` sorts after `end` — Node's own
/// `TypeError` for this one, which `throw_type_error` reaches exactly, no
/// class divergence needed.
extern "C" fn add_range(_e: u64, this: u64, start: u64, end: u64, kind: u64, _d: u64) -> u64 {
    let (Some(start), Some(end)) = (address_text(start), address_text(end)) else {
        invalid_address();
        return entry::undefined_value();
    };
    let (Ok(s), Ok(e)) = (start.parse::<IpAddr>(), end.parse::<IpAddr>()) else {
        invalid_address();
        return entry::undefined_value();
    };
    let inverted = match (s, e) {
        (IpAddr::V4(s), IpAddr::V4(e)) => u32::from(s) > u32::from(e),
        (IpAddr::V6(s), IpAddr::V6(e)) => u128::from(s) > u128::from(e),
        _ => false,
    };
    if inverted {
        entry::throw_type_error(&format!(
            "ERR_INVALID_ARG_VALUE: The argument 'start' must come before end. Received {start}"
        ));
        return entry::undefined_value();
    }
    push_rule(this, format!("Range: {} {start}-{end}", type_arg(kind)));
    entry::undefined_value()
}

extern "C" fn add_subnet(_e: u64, this: u64, net: u64, prefix: u64, kind: u64, _d: u64) -> u64 {
    let Some(net) = address_text(net) else {
        invalid_address();
        return entry::undefined_value();
    };
    let Some(prefix) = entry::number_of(prefix) else { return entry::undefined_value() };
    let max = if type_arg(kind) == "IPv6" { 128 } else { 32 };
    if !(0.0..=max as f64).contains(&prefix) {
        entry::throw_type_error(&format!(
            "ERR_OUT_OF_RANGE: The value of \"prefix\" is out of range. It must be >= 0 && <= {max}. Received {}",
            prefix as i64
        ));
        return entry::undefined_value();
    }
    push_rule(this, format!("Subnet: {} {net}/{}", type_arg(kind), prefix as u32));
    entry::undefined_value()
}

enum Rule {
    Address(IpAddr),
    Range(IpAddr, IpAddr),
    Subnet(IpAddr, u32),
}

fn parse_rule(text: &str) -> Option<Rule> {
    if let Some(rest) = text.strip_prefix("Address: ") {
        let (_, address) = rest.split_once(' ')?;
        return Some(Rule::Address(address.parse().ok()?));
    }
    if let Some(rest) = text.strip_prefix("Range: ") {
        let (_, range) = rest.split_once(' ')?;
        let (start, end) = range.split_once('-')?;
        return Some(Rule::Range(start.parse().ok()?, end.parse().ok()?));
    }
    if let Some(rest) = text.strip_prefix("Subnet: ") {
        let (_, subnet) = rest.split_once(' ')?;
        let (net, prefix) = subnet.split_once('/')?;
        return Some(Rule::Subnet(net.parse().ok()?, prefix.parse().ok()?));
    }
    None
}

fn subnet_contains(net: IpAddr, prefix: u32, candidate: IpAddr) -> bool {
    match (net, candidate) {
        (IpAddr::V4(net), IpAddr::V4(candidate)) => {
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix.min(32)) };
            (u32::from(net) & mask) == (u32::from(candidate) & mask)
        }
        (IpAddr::V6(net), IpAddr::V6(candidate)) => {
            let mask = if prefix == 0 { 0u128 } else { u128::MAX << (128 - prefix.min(128)) };
            (u128::from(net) & mask) == (u128::from(candidate) & mask)
        }
        _ => false,
    }
}

/// `candidate`, and — when it is an IPv4-MAPPED IPv6 address (`::ffff:a.b.c.d`)
/// — the plain IPv4 form beside it, matching Node's own cross-family rule
/// (`src/node_sockaddr.cc`'s `Rule::Match`, this module's own doc names it):
/// an IPv4 rule covers the IPv4-mapped IPv6 shape of the same address. This
/// used to compare `candidate` against a rule of the SAME family only, so a
/// `::ffff:1.2.3.4` candidate never matched an IPv4 rule for `1.2.3.4`.
fn candidate_forms(candidate: IpAddr) -> Vec<IpAddr> {
    match candidate {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => vec![candidate, IpAddr::V4(v4)],
            None => vec![candidate],
        },
        IpAddr::V4(_) => vec![candidate],
    }
}

fn matches(rule: &Rule, candidate: IpAddr) -> bool {
    match rule {
        Rule::Address(address) => *address == candidate,
        Rule::Range(start, end) => match (start, end, candidate) {
            (IpAddr::V4(s), IpAddr::V4(e), IpAddr::V4(c)) => u32::from(c) >= u32::from(*s) && u32::from(c) <= u32::from(*e),
            (IpAddr::V6(s), IpAddr::V6(e), IpAddr::V6(c)) => u128::from(c) >= u128::from(*s) && u128::from(c) <= u128::from(*e),
            _ => false,
        },
        Rule::Subnet(net, prefix) => subnet_contains(*net, *prefix, candidate),
    }
}

extern "C" fn check(_e: u64, this: u64, address: u64, _kind: u64, _c: u64, _d: u64) -> u64 {
    let Some(address) = address_text(address) else { return entry::boolean_value(false) };
    let Ok(candidate) = address.parse::<IpAddr>() else { return entry::boolean_value(false) };
    let forms = candidate_forms(candidate);
    let rules = super::common::get_value(this, "rules");
    let parsed_rules: Vec<Rule> = collect(rules).into_iter().filter_map(entry::text_of).filter_map(|text| parse_rule(&text)).collect();
    let held = parsed_rules.iter().any(|rule| forms.iter().any(|form| matches(rule, *form)));
    entry::boolean_value(held)
}

extern "C" fn to_json(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::common::get_value(this, "rules")
}

extern "C" fn from_json(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let items = if entry::is_array(value) {
        collect(value)
    } else if let Some(text) = entry::text_of(value) {
        vec![entry::with_runtime(|context| entry::make_string(context, &text))]
    } else {
        Vec::new()
    };
    entry::with_runtime(|context| super::common::set_array(context, this, "rules", items));
    entry::undefined_value()
}
