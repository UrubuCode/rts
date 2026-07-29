//! node:net — the `BlockList` class: IP allow/deny rule sets.
//!
//! The rule engine (matching, the canonical rule strings) is [`rules`]; this
//! module is the JS-facing half — the `#[rtse::class]`-backed instance
//! (holding the `Vec<Rule>` directly, no side table), argument normalization,
//! and Node's errors.
//!
//! Node's arguments are polymorphic (`address: string | SocketAddress`), so
//! the members take `Poly` (the raw PolyValue word) and branch on the value —
//! a `SocketAddress` instance is read through its own class data, no string
//! round trip.
//!
//! `BlockList` is not an `EventEmitter` and has no async surface: every member
//! here is a plain synchronous call.

pub mod rules;

use std::net::IpAddr;

use rts_engine::abi::ty::Poly;
use rts_engine::heap::handles::{with_entry, with_rtse, Entry};

use self::rules::{family_of_type, max_prefix, parse_addr, Rule};
use crate::values::{val, Val};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// The JS class name.
pub const CLASS: &str = "BlockList";

/// Throw one of Node's argument errors (its real JS class, so `catch (e)` sees a
/// real `TypeError`/`RangeError` with a `.message` carrying the code).
fn throw(code: &str, class: &str, message: &str) {
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

fn throw_invalid_address(text: &str) {
    throw(
        "ERR_INVALID_ADDRESS",
        "TypeError",
        &format!("Invalid address: '{text}'"),
    );
}

fn throw_invalid_type(kind: &str) {
    throw(
        "ERR_INVALID_ARG_VALUE",
        "TypeError",
        &format!("The argument 'type' is invalid. Received '{kind}'"),
    );
}

/// The address text an argument carries: a plain string, or the `address` of a
/// `SocketAddress` instance (Node accepts either everywhere).
fn address_text(word: u64) -> Option<String> {
    match val(word) {
        Val::Str(s) => Some(s),
        Val::Obj(h) => super::socket_address::address_of(h),
        _ => None,
    }
}

/// Resolve `(addressWord, type)` to a parsed address, throwing Node's error and
/// returning `None` when either is invalid.
fn addr_arg(word: u64, kind: &str) -> Option<IpAddr> {
    let Some(v6) = family_of_type(kind) else {
        throw_invalid_type(kind);
        return None;
    };
    let Some(text) = address_text(word) else {
        throw(
            "ERR_INVALID_ARG_TYPE",
            "TypeError",
            "The \"address\" argument must be a string or a SocketAddress",
        );
        return None;
    };
    match parse_addr(&text, v6) {
        Some(ip) => Some(ip),
        None => {
            throw_invalid_address(&text);
            None
        }
    }
}

/// A `BlockList` instance: just the ordered rule list. The `#[rtse::class]`
/// instance IS this struct (`Entry::Rtse`), so external readers (the `dgram`/
/// `net` server `blockList` option) reach it directly via [`with_rtse`] —
/// [`rules_of`]/[`check_addr`] below are the stable entry points they call.
#[rtse::class("BlockList")]
#[derive(Clone, Default)]
pub struct BlockList {
    rules: Vec<Rule>,
}

#[rtse::class("BlockList")]
impl BlockList {
    /// `new BlockList()`.
    #[rtse::ctor]
    fn new() -> Self {
        BlockList::default()
    }

    /// `blockList.addAddress(address[, type])`.
    #[rtse::method(name = "addAddress", throws)]
    fn add_address(&mut self, address: Poly, kind: Option<&str>) {
        let kind = kind.unwrap_or("ipv4");
        if let Some(ip) = addr_arg(address, kind) {
            self.rules.push(Rule::Address(ip));
        }
    }

    /// `blockList.addRange(start, end[, type])` — Node throws when `start > end`.
    #[rtse::method(name = "addRange", throws)]
    fn add_range(&mut self, start: Poly, end: Poly, kind: Option<&str>) {
        let kind = kind.unwrap_or("ipv4");
        let (Some(start), Some(end)) = (addr_arg(start, kind), addr_arg(end, kind)) else {
            return;
        };
        if !Rule::Range(start, end).matches(start) {
            // A range whose start does not even contain itself is an inverted
            // range (`start > end`) — Node rejects it up front.
            throw(
                "ERR_INVALID_ARG_VALUE",
                "TypeError",
                &format!("The argument 'start' is invalid. Received '{start}' (start > end)"),
            );
            return;
        }
        self.rules.push(Rule::Range(start, end));
    }

    /// `blockList.addSubnet(net, prefix[, type])`.
    #[rtse::method(name = "addSubnet", throws)]
    fn add_subnet(&mut self, network: Poly, prefix: f64, kind: Option<&str>) {
        let kind = kind.unwrap_or("ipv4");
        let Some(ip) = addr_arg(network, kind) else { return };
        let max = max_prefix(ip);
        if !prefix.is_finite() || prefix.fract() != 0.0 || prefix < 0.0 || prefix > f64::from(max) {
            throw(
                "ERR_OUT_OF_RANGE",
                "RangeError",
                &format!(
                    "The value of \"prefix\" is out of range. It must be >= 0 && <= {max}. Received {prefix}"
                ),
            );
            return;
        }
        self.rules.push(Rule::Subnet(ip, prefix as u8));
    }

    /// `blockList.check(address[, type])` — never throws: an address that does
    /// not parse under `type` is simply not in the list.
    #[rtse::method]
    fn check(&self, address: Poly, kind: Option<&str>) -> bool {
        let kind = kind.unwrap_or("ipv4");
        let Some(v6) = family_of_type(kind) else { return false };
        let Some(text) = address_text(address) else { return false };
        let Some(ip) = parse_addr(&text, v6) else { return false };
        self.rules.iter().any(|r| r.matches(ip))
    }

    /// `blockList.rules` (getter) — the rule strings, in insertion order.
    #[rtse::getter]
    fn rules(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.to_rule_string()).collect()
    }

    /// `blockList.toJSON()` — the same `string[]` as `rules`.
    #[rtse::method(name = "toJSON")]
    fn to_json(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.to_rule_string()).collect()
    }

    /// `blockList.fromJSON(value)` — `value` is the `string[]` `toJSON()`
    /// produced, or that array as a JSON string. Rules are ADDED to the
    /// existing set.
    #[rtse::method(name = "fromJSON")]
    fn from_json(&mut self, value: Poly) {
        let bad = || {
            throw(
                "ERR_INVALID_ARG_TYPE",
                "TypeError",
                "The \"data\" argument must be a string, or an array of strings that is JSON parseable",
            )
        };
        let Some(texts) = json_rule_texts(value) else {
            bad();
            return;
        };
        let mut parsed = Vec::with_capacity(texts.len());
        for text in &texts {
            match Rule::parse(text) {
                Some(rule) => parsed.push(rule),
                None => {
                    throw(
                        "ERR_INVALID_ARG_VALUE",
                        "TypeError",
                        &format!("The argument 'data' is invalid. Received '{text}'"),
                    );
                    return;
                }
            }
        }
        self.rules.extend(parsed);
    }

    /// `BlockList.isBlockList(value)` (static) — true only for a real instance.
    #[rtse::statical]
    fn is_block_list(value: Poly) -> bool {
        matches!(val(value), Val::Obj(h) if rts_engine::heap::handles::rtse_class_of(h) == Some(CLASS))
    }
}

/// The rule strings an `fromJSON` argument carries: an array of strings, or a
/// JSON string holding one. `None` = not that shape.
fn json_rule_texts(value: u64) -> Option<Vec<String>> {
    match val(value) {
        Val::Str(json) => {
            // A JSON `["rule", …]` array of strings, parsed by hand: the payload
            // is a flat string array, so a JSON dependency would be overkill.
            let body = json.trim().strip_prefix('[')?.strip_suffix(']')?.trim().to_string();
            if body.is_empty() {
                return Some(Vec::new());
            }
            body.split(',')
                .map(|part| {
                    let part = part.trim();
                    part.strip_prefix('"')?.strip_suffix('"').map(|s| s.to_string())
                })
                .collect()
        }
        Val::Obj(h) => with_entry(h, |e| match e {
            Some(Entry::Vec(words)) => words
                .iter()
                .map(|&w| match val(w as u64) {
                    Val::Str(s) => Some(s),
                    _ => None,
                })
                .collect(),
            _ => None,
        }),
        _ => None,
    }
}

/// Whether any rule covers `addr` — the native side of `check`, also used by
/// the `blockList` options of `node:dgram`/`node:net`.
pub fn check_addr(this: u64, addr: IpAddr) -> bool {
    with_rtse::<BlockList, _>(this, |b| match b {
        Some(b) => b.rules.iter().any(|r| r.matches(addr)),
        None => false,
    })
}

/// The rules of `handle` if it is a `BlockList` — how the `blockList` option of
/// another class (a dgram socket, a net server) reads a list handed to it.
pub fn rules_of(handle: u64) -> Option<Vec<Rule>> {
    with_rtse::<BlockList, _>(handle, |b| b.map(|b| b.rules.clone()))
}
