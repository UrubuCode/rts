//! Properties that are a pair of functions rather than a slot.
//!
//! # Why an accessor is not stored in the shape
//!
//! Because the fast path would find it. Compiled code emits `cached_get`, which
//! reads the slot a layout says a key is at — so a getter recorded as an
//! ordinary property would be *returned* rather than called, the moment the
//! cache started working.
//!
//! That is the same trap [`super::array::set_length`] records from the other
//! side: a special case only the slow path knows about stops applying when the
//! fast path starts. Here the answer is the reverse of the one `length` needed —
//! `length` became a real property so both paths agree, and an accessor must
//! **not** be one, so that `cache_resolve` answers negative and every read of it
//! reaches the runtime.
//!
//! The absence is therefore load-bearing rather than an omission. An accessor
//! key is not in the layout, so a program that later assigns to it through a
//! setter does not silently create a data property beside it.
//!
//! # Why the pair is kept beside the cell
//!
//! For the reason every other side table here exists: an object with an accessor
//! is rare, and seven inline slots are what a program's own properties get. A
//! word per object to record something almost none of them have would be paid by
//! all of them.
//!
//! # Why the call happens outside the borrow
//!
//! A getter is user code, and its first act may be to call the runtime. Calling
//! it from inside a borrow of the context re-enters the `RefCell` — the deadlock
//! this repository has already paid for once. So the lookup answers *which
//! function*, the borrow is released, and the call happens after.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;

/// What a key resolves to on an object with accessors.
///
/// Three answers and not two: a property that is absent, one that holds a value,
/// and one that is a function to call. Collapsing the third into the second is
/// exactly the bug the module documentation describes.
pub(super) enum Found {
    /// A plain value, already read.
    Value(u64),
    /// A function to call, with the receiver to call it on.
    Getter(u64),
    /// Nothing, along the whole chain.
    Absent,
}

/// The getter and setter a cell has for a key, if either.
type Pair = (Option<u64>, Option<u64>);

impl Context {
    /// What a cell defines for a key, if anything.
    ///
    /// A linear scan of a short list rather than a map. An object with
    /// accessors has a handful, and hashing a key would cost more than walking
    /// four entries — the same reasoning `Aside` itself records for using a
    /// `Vec` where the key is dense and small.
    pub(super) fn accessor_at(&self, cell: u32, key: u32) -> Option<Pair> {
        let defined = self.accessors.get(cell)?;
        defined
            .iter()
            .find(|(at, _, _)| *at == key)
            .map(|(_, get, set)| (*get, *set))
    }

    /// Which keys a cell defines accessors for.
    ///
    /// Read by enumeration, which would otherwise report an object holding only
    /// `get x()` as having no properties: the pair is deliberately out of the
    /// layout, so a walk of the shape alone cannot see it.
    pub(super) fn accessors_at(&self, cell: u32) -> Option<Vec<rts_cranelift::shape::Key>> {
        let defined = self.accessors.get(cell)?;
        Some(
            defined
                .iter()
                .filter_map(|(key, _, _)| self.keys.key(*key))
                .collect(),
        )
    }

    /// Records one, keeping whichever half was already there.
    ///
    /// `get x()` and `set x(v)` are two declarations of one property, and a
    /// second definition replacing the pair would make the order they were
    /// written in decide which half survives.
    pub(super) fn define_accessor(
        &mut self,
        cell: u32,
        key: u32,
        get: Option<u64>,
        set: Option<u64>,
    ) {
        if let Some(defined) = self.accessors.get_mut(cell) {
            if let Some(found) = defined.iter_mut().find(|(at, _, _)| *at == key) {
                found.1 = get.or(found.1);
                found.2 = set.or(found.2);
                return;
            }
            defined.push((key, get, set));
            return;
        }
        self.accessors.set(cell, vec![(key, get, set)]);
    }
}

/// `get x() { … }` — records the getter half.
///
/// Two entry points rather than one taking both, because the two halves are
/// written separately and a single call would have to pass `undefined` for the
/// half it does not define — which is indistinguishable from defining it as
/// `undefined`.
#[rtse::entry]
pub fn define_getter(object: u64, key: i64, getter: u64) -> u64 {
    define(object, key, Some(getter), None)
}

/// `set x(v) { … }` — records the setter half.
#[rtse::entry]
pub fn define_setter(object: u64, key: i64, setter: u64) -> u64 {
    define(object, key, None, Some(setter))
}

/// Both, which differ only in which half they carry.
fn define(object: u64, key: i64, get: Option<u64>, set: Option<u64>) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return object;
        };
        let Ok(number) = u32::try_from(key) else {
            return object;
        };
        context.define_accessor(cell, number, get, set);
        object
    })
}

/// Resolves a key over the chain, distinguishing an accessor from a value.
///
/// Walks the accessors and the layouts **together**, one cell at a time, which
/// is the only order that gets shadowing right: an own data property shadows an
/// inherited getter, and an own getter shadows an inherited data property. Two
/// separate walks — accessors first, then values — would let an inherited getter
/// win over an own value.
pub(super) fn resolve(context: &mut Context, start: u32, key: Key) -> Found {
    let Key::Name(machine) = key else {
        return Found::Absent;
    };
    let number = machine.index() as u32;
    let mut cell = start;
    for _ in 0..super::objects::CHAIN_LIMIT {
        if let Some((get, _)) = context.accessor_at(cell, number) {
            return match get {
                Some(getter) => Found::Getter(getter),
                // A setter with no getter reads as `undefined`, which is the
                // language and a common source of confusion — the property
                // exists and reading it answers nothing.
                None => Found::Value(undefined_of(context)),
            };
        }
        if let Some(found) = super::objects::own_property(context, cell, key) {
            return Found::Value(found.bits());
        }
        match super::objects::inherited_from(context, cell) {
            Some(next) => cell = next,
            None => return Found::Absent,
        }
    }
    Found::Absent
}

/// The setter for a key along the chain, if there is one.
///
/// An own **data** property stops the walk: `o.x = 1` on an object that has its
/// own `x` writes the slot, whatever an inherited setter would have done.
pub(super) fn setter_for(context: &mut Context, start: u32, key: Key) -> Option<u64> {
    let Key::Name(machine) = key else {
        return None;
    };
    let number = machine.index() as u32;
    let mut cell = start;
    for _ in 0..super::objects::CHAIN_LIMIT {
        if let Some((_, set)) = context.accessor_at(cell, number) {
            return set;
        }
        if super::objects::own_property(context, cell, key).is_some() {
            return None;
        }
        cell = super::objects::inherited_from(context, cell)?;
    }
    None
}
