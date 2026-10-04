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

/// `new Map()` where the whole program leaves `Map` alone, and `new Set()`.
///
/// # What this removes, measured 2026-10-03
///
/// `new Map()` cost **169 ns** against **47** for `new C()` on a declared class
/// with no fields, and the difference is not the table: `new Object()` — a
/// native constructor with no body worth the name — cost **170** in the same
/// loop, `new Error()` 374 and `new Date(0)` 397. `rts prove` says what it is.
/// A declared class reaches `__rts_object_new_under`, which allocates under a
/// prototype and nothing else; **every native class reaches `__rts_construct`**,
/// the generic door.
///
/// A cumulative ladder over that door priced it: 32 ns to arrive through the two
/// argument stacks, 23 for a constructibility decision a declared class answers
/// from a set, 4 to resolve the callable a second time and push the target, 62
/// for the fresh object, and 69 for the dispatch and the return rule. None of
/// which a zero-argument `new Map()` needs: [`super::fresh`] is the whole
/// operation — a cell, the prototype from the class's own registration, and an
/// empty branded table — and it is the function the door reaches anyway.
///
/// # Why zero arguments only
///
/// `new Map(iterable)` has to iterate, which is a call into user code and the
/// one thing a door earns its cost for. The compiler refuses to emit this for
/// any other form.
///
/// # Why `new.target` cannot be lost here
///
/// Because the compiler emits this only where the callee is the primordial
/// `Map` named directly, which is `new.target === Map` by construction. A
/// subclass reaches `super()`, which is a different operation and still takes
/// the door — so `class Mine extends Map {}` keeps inheriting from
/// `Mine.prototype`.
#[rtse::entry]
pub fn map_new_direct() -> u64 {
    with_current(|context| {
        installed(context, "Map");
        super::fresh(context, "Map")
    })
}

/// Installs the class if nothing has read its name yet.
///
/// # Why this is here and not in [`super::fresh`]
///
/// `entry::global::supply` is what installs `Map` and `Set`, and it runs on the
/// first READ of the global -- so the registration `fresh` needs exists only
/// because something mentioned the name first. Its three other callers satisfy
/// that by accident: `Map.groupBy` and the `Set` combinators are reached THROUGH
/// the name, and the constructor path takes its prototype from
/// `functions::allocate_for` rather than from the registry.
///
/// These two entry points break the accident, because removing the global read
/// is half of what they remove. Without this the prototype was never set and
/// `fresh`'s `if let` simply skipped: a Map with a working table, answering
/// `m.get(k)` correctly while `m instanceof Map` was FALSE, `m.size` was empty
/// and `m.get` was `undefined` -- a wrong answer that runs, measured 2026-10-03
/// and caught by `tests/map_set_size_chain.test.ts`.
///
/// Putting it in `fresh` was the first fix and is wrong for a measurable
/// reason: the pickle's name registry is a `Map` built through `fresh`, so
/// installing from there made every program that declares a name install the
/// whole class -- the shape tree grew by 19 layouts where it grew by 3, which
/// `pickle::names_tests::a_declaration_makes_no_layout_of_its_own` reports as a
/// COUNT and not as a clock. The need belongs to the caller that created it.
fn installed(context: &mut crate::entry::Context, class: &str) {
    if super::super::class_support::prototype(context, class).is_some() {
        return;
    }
    // Through `machine_key`, because `supply` is keyed by the machine's
    // numbering while `well_known` answers this crate's own key type -- an
    // index is not a name and cannot install a class.
    if let Some(named) = super::super::objects::machine_key(context.well_known(class)) {
        super::super::global::supply(context, named);
    }
}

/// `new Set()` on the same terms. See [`map_new_direct`].
#[rtse::entry]
pub fn set_new_direct() -> u64 {
    with_current(|context| {
        installed(context, "Set");
        super::fresh(context, "Set")
    })
}
