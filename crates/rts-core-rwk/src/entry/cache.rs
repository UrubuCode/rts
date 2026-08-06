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

use super::with_current;

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
    with_current(|context| {
        context.resolves += 1;
        let Ok(number) = u32::try_from(key) else {
            return -1;
        };
        let Some(key) = context.keys.key(number) else {
            return -1;
        };
        let Some(ty) = context.region.type_of(object as u32) else {
            return -1;
        };
        let Some(shape) = context.shape_of(ty) else {
            // A string, or a layout nothing recorded. Not an object, so nothing
            // is at any offset in it.
            return -1;
        };
        let Some(slot) = context.shapes.slot_of(shape, key) else {
            // Absent. Legal, and it reads as `undefined` — but not by loading,
            // which is all this answer says.
            return -1;
        };
        if slot >= crate::heap::INLINE_SLOTS {
            // Past the inline slots, where the overflow indirection will go.
            // Until it exists, the slow path is the only correct answer.
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
        u32::try_from(key)
            .ok()
            .and_then(|number| context.keys.key(number))
            .is_some_and(|key| super::integrity::refuses_key_write(context, object as u32, key))
    });
    if refused {
        return -1;
    }
    cache_resolve(object, key, cache)
}
