//! node:net — the `BlockList` class: IP allow/deny rule sets.
//!
//! The rule engine (matching, the canonical rule strings) is [`rules`]; this
//! module is the JS-facing half — the object-backed Registry class (an
//! `Entry::Map` tagged `__rts_class = "BlockList"`, whose handle keys the rule
//! table here), argument normalization, and Node's errors.
//!
//! Node's arguments are polymorphic (`address: string | SocketAddress`), so the
//! members take `AbiType::PolyValue` and branch on the value — a `SocketAddress`
//! instance is read through its own class data, no string round trip.
//!
//! `BlockList` is not an `EventEmitter` and has no async surface: every member
//! here is a plain synchronous call.

pub mod rules;

use std::net::IpAddr;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use self::rules::{family_of_type, max_prefix, parse_addr, Rule};
use crate::values::{read, string_array, val, Val};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// The JS class name — also the `__rts_class` tag every instance carries.
pub const CLASS: &str = "BlockList";

type Table = indexmap::IndexMap<u64, Vec<Rule>>;

fn table() -> MutexGuard<'static, Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Table::new())).lock().unwrap()
}

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

/// `new BlockList()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_NEW() -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(CLASS.as_bytes().to_vec())) as i64,
    );
    let handle = alloc_entry(Entry::Map(Box::new(m)));
    table().insert(handle, Vec::new());
    handle
}

/// Whether `handle` is a live `BlockList` instance — the shape check behind both
/// `BlockList.isBlockList` and every instance method.
fn is_block_list(handle: u64) -> bool {
    table().contains_key(&handle)
}

fn push(this: u64, rule: Rule) {
    if let Some(list) = table().get_mut(&this) {
        list.push(rule);
    }
}

/// `blockList.addAddress(address[, type])`.
fn add_address(this: u64, address: u64, kind: &str) {
    if let Some(ip) = addr_arg(address, kind) {
        push(this, Rule::Address(ip));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS(this: u64, a: u64) {
    add_address(this, a, "ipv4");
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS_T(this: u64, a: u64, tp: *const u8, tl: i64) {
    add_address(this, a, &read(tp, tl));
}

/// `blockList.addRange(start, end[, type])` — Node throws when `start > end`.
fn add_range(this: u64, start: u64, end: u64, kind: &str) {
    let (Some(start), Some(end)) = (addr_arg(start, kind), addr_arg(end, kind)) else {
        return;
    };
    if !Rule::Range(start, end).matches(start) {
        // A range whose start does not even contain itself is an inverted range
        // (`start > end`) — Node rejects it up front.
        throw(
            "ERR_INVALID_ARG_VALUE",
            "TypeError",
            &format!("The argument 'start' is invalid. Received '{start}' (start > end)"),
        );
        return;
    }
    push(this, Rule::Range(start, end));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE(this: u64, s: u64, e: u64) {
    add_range(this, s, e, "ipv4");
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE_T(
    this: u64,
    s: u64,
    e: u64,
    tp: *const u8,
    tl: i64,
) {
    add_range(this, s, e, &read(tp, tl));
}

/// `blockList.addSubnet(net, prefix[, type])`.
fn add_subnet(this: u64, network: u64, prefix: f64, kind: &str) {
    let Some(ip) = addr_arg(network, kind) else { return };
    let max = max_prefix(ip);
    if !prefix.is_finite() || prefix.fract() != 0.0 || prefix < 0.0 || prefix > f64::from(max) {
        throw(
            "ERR_OUT_OF_RANGE",
            "RangeError",
            &format!("The value of \"prefix\" is out of range. It must be >= 0 && <= {max}. Received {prefix}"),
        );
        return;
    }
    push(this, Rule::Subnet(ip, prefix as u8));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET(this: u64, n: u64, prefix: f64) {
    add_subnet(this, n, prefix, "ipv4");
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET_T(
    this: u64,
    n: u64,
    prefix: f64,
    tp: *const u8,
    tl: i64,
) {
    add_subnet(this, n, prefix, &read(tp, tl));
}

/// Whether any rule covers `addr` — the native side of `check`, also used by the
/// `blockList` options of `node:dgram`/`node:net`.
pub fn check_addr(this: u64, addr: IpAddr) -> bool {
    match table().get(&this) {
        Some(rules) => rules.iter().any(|r| r.matches(addr)),
        None => false,
    }
}

/// `blockList.check(address[, type])` — never throws: an address that does not
/// parse under `type` is simply not in the list.
fn check(this: u64, address: u64, kind: &str) -> i64 {
    let Some(v6) = family_of_type(kind) else { return 0 };
    let Some(text) = address_text(address) else { return 0 };
    let Some(ip) = parse_addr(&text, v6) else { return 0 };
    i64::from(check_addr(this, ip))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_CHECK(this: u64, a: u64) -> i64 {
    check(this, a, "ipv4")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_CHECK_T(this: u64, a: u64, tp: *const u8, tl: i64) -> i64 {
    check(this, a, &read(tp, tl))
}

/// The rule strings, in insertion order — `blockList.rules` and `toJSON()`.
fn rule_strings(this: u64) -> Vec<String> {
    table()
        .get(&this)
        .map(|rules| rules.iter().map(|r| r.to_rule_string()).collect())
        .unwrap_or_default()
}

/// `blockList.rules` (getter) / `blockList.toJSON()` — the same `string[]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_RULES(this: u64) -> u64 {
    string_array(&rule_strings(this))
}

/// `blockList.fromJSON(value)` — `value` is the `string[]` `toJSON()` produced,
/// or that array as a JSON string. Rules are ADDED to the existing set.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_FROM_JSON(this: u64, value: u64) {
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
    if let Some(list) = table().get_mut(&this) {
        list.extend(parsed);
    }
}

/// The rule strings a `fromJSON` argument carries: an array of strings, or a
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

/// `BlockList.isBlockList(value)` (static) — true only for a real instance.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_BLOCKLIST_IS_BLOCK_LIST(value: u64) -> i64 {
    match val(value) {
        Val::Obj(h) => i64::from(is_block_list(h)),
        _ => 0,
    }
}

/// The rules of `handle` if it is a `BlockList` — how the `blockList` option of
/// another class (a dgram socket, a net server) reads a list handed to it.
pub fn rules_of(handle: u64) -> Option<Vec<Rule>> {
    table().get(&handle).cloned()
}
