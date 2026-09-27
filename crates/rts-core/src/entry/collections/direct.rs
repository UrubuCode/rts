//! `Map.get`, `Map.has`, `Map.set`, `Set.has`, `Set.add`, reached directly.
//!
//! Each of these cost 42 to 68 ns as a call — a property read through the chain
//! cache, a native dispatch, two borrows, three stacks — for a body that is one
//! hash probe (`docs/codegen/native-call-floor.md`). Where the whole program
//! leaves `Map` and `Set` as the language defines them, the compiler emits one
//! of these instead: the BRAND is checked first, a real instance answers from
//! its table, and anything else — an object with its own `get`, a proxy — takes
//! `direct_call::through_the_method`, which is the call the compiler would have
//! emitted. A subclass overriding `get` never reaches here at all: `extends Map`
//! is `Map` used as a value, which ends the proof for the whole program.
//!
//! The bodies are the members' own (`map.rs`, `set.rs`), so the direct form and
//! the method cannot disagree about a key, a hole or a return value.

use super::map::held;
use super::{Brand, branded_quietly, restore_sized, taken};
use crate::entry::direct_call::through_the_method;
use crate::entry::objects::undefined_of;
use crate::entry::with_current;
use crate::value::Value;

/// `m.get(k)`.
#[rtse::entry]
pub fn map_get_direct(this: u64, key: u64, name: i64) -> u64 {
    let Some(cell) = branded_quietly(this, Brand::Map) else {
        return through_the_method(this, "get", name, &[key]);
    };
    with_current(|context| {
        let absent = undefined_of(context);
        match context.table_at(cell) {
            Some(table) => table.get(context, key).unwrap_or(absent),
            None => absent,
        }
    })
}

/// `m.has(k)` — and `s.has(v)`, because the compiler cannot tell a `Map` from a
/// `Set` by the member's name and routes every `.has(x)` here when both are
/// primordial. Either brand answers from its table (`held` reads both); refusing
/// the `Set` sent it through the generic call at 128 ns against its 35.
#[rtse::entry]
pub fn map_has_direct(this: u64, key: u64, name: i64) -> u64 {
    if branded_quietly(this, Brand::Map).is_none() && branded_quietly(this, Brand::Set).is_none() {
        return through_the_method(this, "has", name, &[key]);
    }
    Value::from_bool(with_current(|context| held(context, this, key))).bits()
}

/// `m.set(k, v)` — the map, so that writes chain.
#[rtse::entry]
pub fn map_set_direct(this: u64, key: u64, value: u64, name: i64) -> u64 {
    let Some(cell) = branded_quietly(this, Brand::Map) else {
        return through_the_method(this, "set", name, &[key, value]);
    };
    with_current(|context| {
        if let Some(mut table) = taken(context, cell) {
            table.set(context, key, value);
            restore_sized(context, cell, table);
        }
        this
    })
}

/// `s.has(v)`.
#[rtse::entry]
pub fn set_has_direct(this: u64, value: u64, name: i64) -> u64 {
    if branded_quietly(this, Brand::Set).is_none() {
        return through_the_method(this, "has", name, &[value]);
    }
    Value::from_bool(with_current(|context| held(context, this, value))).bits()
}

/// `s.add(v)` — the set, so that writes chain.
#[rtse::entry]
pub fn set_add_direct(this: u64, value: u64, name: i64) -> u64 {
    let Some(cell) = branded_quietly(this, Brand::Set) else {
        return through_the_method(this, "add", name, &[value]);
    };
    let value = super::table::canonical(value);
    with_current(|context| {
        if let Some(mut table) = taken(context, cell) {
            table.set(context, value, value);
            restore_sized(context, cell, table);
        }
        this
    })
}
