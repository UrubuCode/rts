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

use rts_core_rwk::entry::{self, Provided};
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

extern "C" fn add_address(_e: u64, this: u64, address: u64, kind: u64, _c: u64, _d: u64) -> u64 {
    let Some(address) = entry::text_of(address) else { return entry::undefined_value() };
    push_rule(this, format!("Address: {} {address}", type_arg(kind)));
    entry::undefined_value()
}

/// `blockList.addRange(start, end, type?)`. Four call slots total once the
/// receiver takes one — `type` reads from the last, matching the ceiling
/// this task's brief states.
extern "C" fn add_range(_e: u64, this: u64, start: u64, end: u64, kind: u64, _d: u64) -> u64 {
    let Some(start) = entry::text_of(start) else { return entry::undefined_value() };
    let Some(end) = entry::text_of(end) else { return entry::undefined_value() };
    push_rule(this, format!("Range: {} {start}-{end}", type_arg(kind)));
    entry::undefined_value()
}

extern "C" fn add_subnet(_e: u64, this: u64, net: u64, prefix: u64, kind: u64, _d: u64) -> u64 {
    let Some(net) = entry::text_of(net) else { return entry::undefined_value() };
    let Some(prefix) = entry::number_of(prefix) else { return entry::undefined_value() };
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
    let Some(address) = entry::text_of(address) else { return entry::boolean_value(false) };
    let Ok(candidate) = address.parse::<IpAddr>() else { return entry::boolean_value(false) };
    let rules = super::common::get_value(this, "rules");
    let held = collect(rules).into_iter().filter_map(entry::text_of).filter_map(|text| parse_rule(&text)).any(|rule| matches(&rule, candidate));
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
