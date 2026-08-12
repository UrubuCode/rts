//! Filling a read site's memory of the layout it last saw.
//!
//! # What a cached read actually asks
//!
//! Not "what is this property". The site already loaded, or tried to: it
//! compared the object's type against the one it remembers and, when they
//! matched, read at the offset it remembers. This is what it calls when they did
//! not match.
//!
//! So the answer is a **byte offset**, not a value. The site writes it into its
//! cell and reads at it, which is why there is one load rather than one on each
//! path — the machine's own comment on `CacheResolve` says so.
//!
//! # Why a negative number is the failure
//!
//! Because a byte offset cannot be negative and the signature returns one
//! number. A property that is absent, or held somewhere a machine word does not
//! reach — an accessor, an index — answers below zero, and the site takes its
//! slow path.
//!
//! It is not an error. `o.missing` is legal and produces `undefined`; what the
//! negative says is only that this read cannot be done by loading.

use super::{Context, with_current};

/// Where a property sits in an object, for the site that just missed.
///
/// # The cell is written HERE, and getting that wrong is what this cost
///
/// The first version named the parameter `_cache` and left a comment saying the
/// machine fills the cell. The machine's own documentation says the opposite,
/// and I had read it:
///
/// > Refills the cell, so that the load after it reads the answer this call
/// > just wrote — which is why there is one load rather than one on each path.
///
/// The lowering bears it out: after asking, it branches straight to the load,
/// which reads the offset out of the cell. A cell nobody wrote holds zero, so
/// the load read at offset zero — the header — and every property came back as
/// its own object's type number.
///
/// What the caller is spared is different and still true: there is no cell to
/// initialize, no decision about when to update, and no way for a site to be
/// told wrong. The cell has one writer and this is it.
///
/// # The layout of a cell
///
/// Two words, and it is the lowering that fixes them: the type it compares
/// against at offset 0, the byte offset it loads at at offset 8.
#[rtse::entry("rts_cache_resolve")]
pub fn cache_resolve(object: u64, key: i64, cache: i64) -> i64 {
    super::string::probe_resolves();
    with_current(|context| {
        context.resolves += 1;
        let explain = |why: &str, context: &mut Context| {
            // A miss is ORDINARY the first time a site sees a layout — that is
            // what a cache is. What this exists to catch is the site that
            // misses forever, so it reports the reason and the key rather than
            // a count, and stops after twenty so a real program stays readable.
            if (context.resolves <= 20 || context.resolves % 200_000 == 0) && std::env::var_os("RTS_CACHE_DEBUG").is_some() {
                let named = u32::try_from(key)
                    .ok()
                    .and_then(|number| context.keys.key(number))
                    .and_then(|key| context.interner.text(key).and_then(|text| text.to_rust()))
                    .unwrap_or_else(|| "?".to_owned());
                eprintln!(
                    "rts-cache miss #{} key {named} cell {cache:#x}: {why}",
                    context.resolves
                );
            }
        };
        let Ok(number) = u32::try_from(key) else {
            explain("the key does not fit a number", context);
            return -1;
        };
        let Some(key) = context.keys.key(number) else {
            explain("no key registered under that number", context);
            return -1;
        };
        let Some(ty) = context.region.type_of(object as u32) else {
            explain("the receiver is not a cell in this region", context);
            return -1;
        };
        let Some(shape) = context.shape_of(ty) else {
            // A string, or a layout nothing recorded. Not an object, so nothing
            // is at any offset in it.
            explain("the receiver has no shape: a string, or a layout nothing recorded", context);
            return -1;
        };
        let Some(slot) = context.shapes.slot_of(shape, key) else {
            // Absent. Legal, and it reads as `undefined` — but not by loading,
            // which is all this answer says.
            explain("the property is absent from the receiver's shape", context);
            return -1;
        };
        if slot >= crate::heap::INLINE_SLOTS {
            // Past the inline slots, where the overflow indirection will go.
            // Until it exists, the slow path is the only correct answer.
            explain("the slot is past the inline slots", context);
            return -1;
        }

        // Past the header, then the slot. The same arithmetic the layout does,
        // and it is here rather than derived from `ObjectLayout` because the
        // cell's inline slots are this runtime's own shape — a layout describes
        // an aggregate, and a cell is an aggregate in a fixed-size box.
        let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
            + i64::from(slot) * i64::from(rts_cranelift::mem::SLOT_BYTES);

        // SAFETY: `cache` is the address of a two-word cell the compilation
        // allocated for this read site and keeps alive for as long as the code
        // is. The lowering passes it and then loads from it, so writing it here
        // is the contract rather than an intrusion.
        unsafe {
            let cell = cache as *mut i64;
            cell.write(i64::from(ty));
            cell.add(1).write(offset);
        }
        offset
    })
}

/// Where a property is, for a site that may read it out of the cell the
/// receiver inherits from.
///
/// # What it may answer that [`cache_resolve`] may not
///
/// An address. The cell is four words rather than two — the receiver's type, the
/// byte offset, **the address to read at** (zero meaning the receiver itself),
/// and the type that address carried when this answered. The machine compares
/// both types and loads at the offset; it never walks anything, and it has no
/// concept of a chain. Everything below decides what is safe to remember.
///
/// # Why every refusal here is explicit, where `cache_resolve`'s were incidental
///
/// Because that one could not reach anything but the receiver's own layout, so a
/// proxy, an accessor and an inherited property were all uncacheable by
/// construction — `proxy.rs` states exactly that as the reason nothing in the
/// fast path had to change for proxies. A resolver allowed to look further
/// destroys that argument, so each case it must not answer for is refused by
/// name and reports why under `RTS_CACHE_DEBUG`.
///
/// # One step, and the reason it is one
///
/// A property found two links away would be an address this site cannot argue is
/// alive. At one step the argument holds: the receiver's type is discriminated
/// by its link (`Context::typed_as`), so recognising the type proves the link is
/// the cell whose address was remembered, and a live receiver keeps its link
/// alive through `trace`. At two steps the middle cell's own link can be
/// reassigned, and nothing the site compares would notice. So depth is one, and
/// closing that is a separate change with a separate argument to make.
///
/// # A deeper property does NOT keep missing as it did, and the reason it must
/// not
///
/// That is what this said, and it was measured false. `derived.bp()` over a
/// `class Derived extends Base` is two links away, so this resolver refuses it
/// — and it refused it once per call: 200 000 refusals in 200 000 calls under
/// `RTS_CACHE_DEBUG`. The site paid the whole attempt — the proxy question, the
/// accessor question, `type_of`, `shape_of`, `slot_of` — and then paid the
/// generic lookup it was already paying. The line doubled, 275 ns to 512 ns,
/// which is the one thing a cache may never do.
///
/// So a refusal it can argue is stable is REMEMBERED: the cell gets the
/// receiver's type, a negative offset, and the link it looked at. The machine
/// checks the sign before it loads, and a site whose answer is not reachable by
/// loading stops calling here at all.
///
/// # Why a remembered negative needs no invalidation this did not already have
///
/// Because it is not a remembered answer. Every other entry in this cell
/// SUBSTITUTES for the lookup, so a stale one is a wrong program. A negative
/// substitutes for nothing: it selects the miss path, which is
/// `RuntimeOp::GetProperty`, the general lookup that consults the whole chain
/// at the moment it runs. A negative that has gone stale is therefore slow and
/// never wrong, and the question "what makes a `bp` that has appeared visible
/// again?" has the answer "the miss path, which never stopped looking".
///
/// What the two type comparisons buy is the speed BACK, and they are the ones
/// already there. Give the receiver the property and its shape transitions, so
/// its type changes and word 0 stops matching. Give the LINK the property and
/// the link's type changes, so word 3 stops matching — which is why a
/// remembered negative records the link it consulted rather than zero.
/// Reassign the receiver's link and `chain::set_prototype` retypes the cell on
/// the spot, which is the same one word 0 compares — so that case is covered by
/// a mechanism that was already there for the positives.
///
/// What none of the three notices is the LINK's own link being reassigned
/// (`Derived.prototype.__proto__ = other`), which is exactly the step this
/// resolver may not walk. Such a site stays on the general lookup, at the speed
/// it had before any cache existed, and never at a wrong answer. Stated here
/// rather than left to be discovered.
///
/// The liveness argument for the address written with a negative is the one
/// above, unchanged and for the same reason: the address recorded is the
/// receiver's own link, which the receiver's type discriminates and a live
/// receiver keeps alive.
///
/// Refusals that are facts about ONE cell rather than about its type — a proxy,
/// an own accessor — are not remembered. They would arm a negative under a type
/// its siblings share, and slow down cells the refusal was never about.
#[rtse::entry("rts_cache_resolve_indirect")]
pub fn cache_resolve_indirect(object: u64, key: i64, cache: i64) -> i64 {
    super::string::probe_resolves_indirect();
    let own = cache_resolve(object, key, cache);
    if own >= 0 {
        // The receiver had it. The machine reads the third word to decide where
        // to load from, and `cache_resolve` did not write one — so it is written
        // here rather than left holding whatever the last answer put there.
        //
        // SAFETY: the same cell `cache_resolve` was just handed and wrote two
        // words of; this site declared four because its terminator is the
        // indirect one.
        unsafe {
            let cell = cache as *mut i64;
            cell.add(2).write(0);
            cell.add(3).write(-1);
        }
        return own;
    }

    with_current(|context| {
        // Every refusal below says which one it was, for the reason
        // `cache_resolve`'s do: a site that misses forever looks exactly like a
        // site that has not run yet, and the only difference visible from
        // outside is a count. This resolver has ten ways to decline and finding
        // out which took a rebuild the first time it was needed.
        let report = |why: &str| -> i64 {
            if std::env::var_os("RTS_CHAIN_DEBUG").is_some() {
                eprintln!("rts-chain refused: {why}");
            }
            -1
        };
        // The receiver arrives as a PROVEN reference, which is the cell number
        // itself and not a tagged value — the signature says
        // `Repr::Ref(RefKind::Opaque)` and the guard before the terminator is
        // what narrowed it. `cache_resolve` reads it the same way one line into
        // its own body; reading it as a boxed value instead answers "not a
        // cell" for every receiver there has ever been, which is what this did
        // for exactly one build.
        let cell = object as u32;
        let Ok(number) = u32::try_from(key) else {
            return report("the key does not fit a number");
        };
        let Some(named) = context.keys.key(number) else {
            return report("no key registered under that number");
        };

        // A proxy answers through its handler, and answering from a layout would
        // be answering instead of it.
        if context.proxy_at(cell).is_some() {
            return report("the receiver is a proxy");
        }
        // An own accessor shadows anything inherited, and `accessor::resolve` is
        // the one implementation of that order. Refusing here keeps it the one.
        if context.accessors.get(cell).is_some() {
            return report("the receiver has accessors of its own");
        }

        // No recorded link, no walk. This is not an optimisation: `inherited_from`
        // SUBSTITUTES a prototype by kind for arrays, callables, text and plain
        // objects, so those cells share one undiscriminated layout — and a site
        // that cached against one would recognise every other. An array and an
        // object literal holding `length` reach the same shape and the same type
        // today, which is exactly the collision this refusal removes.
        let Some(receiver_type) = context.region.type_of(cell) else {
            return report("the receiver is not a cell in this region");
        };
        // Remembering a refusal needs the receiver's type, so it is read here
        // rather than at the end, where it was read only to write a hit.
        let remember = |why: &str, held: u64, held_type: i64| -> i64 {
            if std::env::var_os("RTS_CHAIN_DEBUG").is_some() {
                eprintln!("rts-chain refused and remembered: {why}");
            }
            // SAFETY: the four-word cell this site's terminator declared, the
            // same one the answering path writes.
            unsafe {
                let cell = cache as *mut i64;
                cell.write(i64::from(receiver_type));
                cell.add(1).write(-1);
                cell.add(2).write(held as i64);
                cell.add(3).write(held_type);
            }
            -1
        };

        let Some(link) = context.prototype_at(cell) else {
            // Nothing to look at, and nothing that could make one appear
            // without changing this receiver's type — `typed_as` gives a linked
            // cell a number of its own. So there is no second cell to record.
            return remember("the receiver has no recorded link", 0, -1);
        };
        let Some(holder) = crate::value::Value(link).as_slot() else {
            return report("the link is not a cell");
        };
        if context.proxy_at(holder).is_some() {
            return report("the link is a proxy");
        }
        let Ok(named_number) = u32::try_from(named.index()) else {
            return report("the key number does not fit");
        };
        if context.accessor_at(holder, named_number).is_some() {
            return report("the link holds an accessor for this key");
        }

        let Some(holder_type) = context.region.type_of(holder) else {
            return report("the link is not a cell in this region");
        };
        let Some(holder_shape) = context.shape_of(holder_type) else {
            return report("the link has no shape");
        };
        let Some(address) = context.region.address_of(holder) else {
            return report("the link has no address");
        };
        let Some(slot) = context.shapes.slot_of(holder_shape, named) else {
            // The regression this whole mechanism exists for: `bp` lives one
            // link further on, which this resolver may not reach. Recording the
            // link means the day it gains `bp` its type changes and the site
            // asks again.
            return remember(
                "the key is absent from the link's shape",
                address,
                i64::from(holder_type),
            );
        };
        if slot >= crate::heap::INLINE_SLOTS {
            return report("the slot is past the inline slots");
        }

        let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
            + i64::from(slot) * i64::from(rts_cranelift::mem::SLOT_BYTES);

        // SAFETY: a four-word cell this site's terminator declared, kept alive
        // for as long as the code is.
        unsafe {
            let cell = cache as *mut i64;
            cell.write(i64::from(receiver_type));
            cell.add(1).write(offset);
            cell.add(2).write(address as i64);
            cell.add(3).write(i64::from(holder_type));
        }
        offset
    })
}

/// The same question, from a site that is about to **write**.
///
/// # Why a frozen object answers negative instead of being caught later
///
/// Because there is no later. A site that gets an offset writes at it on every
/// subsequent pass without asking again — so the only place a store to a frozen
/// object can be stopped is before the site remembers where it would write.
///
/// Refusing here sends the store to its miss path, which is
/// [`super::objects::put`], which reads the integrity table and does nothing.
/// [`cache_resolve`] still answers an offset for the same property, which is
/// what keeps a frozen object's properties readable at full speed — and the
/// reason the machine has two entry points rather than one with a flag.
#[rtse::entry("rts_cache_resolve_store")]
pub fn cache_resolve_store(object: u64, key: i64, cache: i64) -> i64 {
    let refused = with_current(|context| {
        let Some(key) = u32::try_from(key).ok().and_then(|number| context.keys.key(number))
        else {
            return false;
        };
        if super::integrity::refuses_key_write(context, object as u32, key) {
            return true;
        }
        // An ARRAY's `length` is refused too, and not because writing it is
        // forbidden — because it is not a plain store. `a.length = 1` truncates
        // the elements, so the write has to reach the slow path where
        // `objects::put` can reconcile them. Cached, it stored the number and
        // left the array disagreeing with itself: `length` answered 1 while
        // `a[1]` still answered what was there.
        //
        // Only an array pays. Everything else keeps `length` as the ordinary
        // cacheable property it is.
        let length = match super::computed::length_key(context) {
            crate::object::Key::Name(named) => named,
            crate::object::Key::Index(_) => return false,
        };
        key == length && context.elements_at(object as u32).is_some()
    });
    if refused {
        return -1;
    }
    cache_resolve(object, key, cache)
}
