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
        // The whole header word, because that is what the machine compares
        // against: it loads the header and checks it equals what this site
        // remembered. The header carries the cell's WIDTH beside its type, so
        // remembering the type alone would compare a masked value against an
        // unmasked one and never match again. It also means a site warmed on a
        // fifteen-slot object declines a wider one of the same shape — a miss,
        // which is safe, rather than an offset read out of the wrong cell size.
        let Some(remembered) = context.region.header_of(object as u32) else {
            explain("the receiver is not a cell in this region", context);
            return -1;
        };
        // A string has no shape, and that absence is load-bearing: `shape_of`
        // excludes the text layout so a reserved position cannot answer with a
        // shape that was never its own. So `length` — the one property a string
        // has that a LOAD could answer — could never be cached, and every read
        // went to the runtime forever at 99 ns against 4.8 for an ordinary one.
        //
        // Answered here rather than by giving text a shape, which is the change
        // that would also make every OTHER property of a string resolve against
        // a layout it does not have. One key, one slot, stated where the cache
        // is filled.
        // `length_key` answers this crate's own `Key`, which distinguishes a
        // name from an index; the cache compares the machine's, which is a
        // name and nothing else. Unwrapping here rather than widening either
        // side keeps the two vocabularies apart, the way `cache_resolve_store`
        // already does for the same key.
        let length_named = match super::computed::length_key(context) {
            crate::object::Key::Name(named) => Some(named),
            crate::object::Key::Index(_) => None,
        };
        if ty == context.text_type_index() && Some(key) == length_named {
            let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
                + i64::from(super::TEXT_LENGTH_SLOT) * i64::from(rts_cranelift::mem::SLOT_BYTES);
            // SAFETY: the cell this site declared, as everywhere else here.
            unsafe {
                let cell = cache as *mut i64;
                cell.write(remembered as i64);
                cell.add(1).write(offset);
            }
            return offset;
        }
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
        // The bound is the CELL's width and not a constant fifteen. A wide
        // object — one the emitter sized to its shape at creation — owns every
        // slot contiguously past its header, so the fortieth is the same load
        // as the second and needs no indirection to reach. That is the whole
        // reason the width is in the header.
        let width = context
            .region
            .width_of(object as u32)
            .unwrap_or(crate::heap::INLINE_SLOTS);
        if slot >= width {
            // Genuinely out of the cell: the property lives in the overflow,
            // which no load can reach.
            explain("the slot is past the slots this cell owns", context);
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
            cell.write(remembered as i64);
            cell.add(1).write(offset);
        }
        offset
    })
}

/// What a refused chain site keeps in the word the machine compares against a
/// holder's header.
///
/// Negative, so it can never equal a real type number and the machine's own
/// comparison stays safe; distinct from the cold `-1` so that a site which has
/// merely never run is told apart from one the walk has already declined.
const REFUSED: i64 = -2;

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
/// reassigned, and nothing the site compares would notice. So depth is one, a
/// deeper property keeps missing exactly as it does today, and closing that is a
/// separate change with a separate argument to make.
#[rtse::entry("rts_cache_resolve_indirect")]
pub fn cache_resolve_indirect(object: u64, key: i64, cache: i64) -> i64 {
    // ONE crossing, not two. This called `cache_resolve` first and then did its
    // own `with_current`, so every site that ends up refused — every method two
    // links away, which is most class code — paid the thread-local, the borrow
    // and the key resolution TWICE on every execution, forever. Measured: a
    // depth-2 method call went 1754 -> 3709 ms over 1e7 calls, 2.1x, against the
    // binary from before this entry point existed.
    //
    // The own-property answer is computed here rather than delegated for that
    // reason alone. It is the same three lookups `cache_resolve` performs and
    // they are performed once.
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

        // The receiver's own layout first, which is what the overwhelming
        // majority of sites want and what `cache_resolve` answers. Written out
        // rather than delegated so that a refusal costs one crossing.
        if let Some(ty) = context.region.type_of(cell)
            && let Some(header) = context.region.header_of(cell)
            && let Some(width) = context.region.width_of(cell)
            && let Some(shape) = context.shape_of(ty)
            && let Some(slot) = context.shapes.slot_of(shape, named)
            && slot < width
        {
            let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
                + i64::from(slot) * i64::from(rts_cranelift::mem::SLOT_BYTES);
            // SAFETY: the four-word cell this site's terminator declared. Word
            // two is zero because the answer is in the cell asked about, and
            // word three cannot match any real header, so a cell claiming an
            // address it never got could not also match a layout.
            unsafe {
                let cell = cache as *mut i64;
                cell.write(header as i64);
                cell.add(1).write(offset);
                cell.add(2).write(0);
                cell.add(3).write(-1);
                cell.add(4).write(0);
                cell.add(5).write(-1);
            }
            return offset;
        }

        // Has this site already been told the walk cannot answer for it?
        //
        // Without this the walk runs on EVERY execution of a site it can never
        // serve — eight lookups to reach the same refusal — and that is most
        // class code, because a method two links away is refused here. Measured
        // before the marker existed: a depth-2 method call cost 3591 ms over 1e7
        // calls against 1789 for the same program compiled before this entry
        // point existed. The walk was the difference, not the extra guard.
        //
        // The marker lives in the word the machine compares against a HEADER, so
        // it is unreachable as a false match: a header is a type number and this
        // is negative. It is written only after a refusal, and it is checked
        // AFTER the own attempt above — a site whose receiver later grows the
        // property still resolves, and only the chain walk is given up on.
        //
        // What it costs: a site that becomes chain-resolvable later never finds
        // out. Slower, never wrong, and the case is a program that moves a method
        // between prototypes after the site has run.
        // SAFETY: the four-word cell this site's terminator declared.
        if unsafe { (cache as *const i64).add(3).read() } == REFUSED {
            return -1;
        }
        let refuse = |why: &str| -> i64 {
            // SAFETY: as above.
            unsafe { (cache as *mut i64).add(3).write(REFUSED) };
            report(why)
        };

        // A proxy answers through its handler, and answering from a layout would
        // be answering instead of it.
        if context.proxy_at(cell).is_some() {
            return refuse("the receiver is a proxy");
        }
        // An own accessor shadows anything inherited, and `accessor::resolve` is
        // the one implementation of that order. Refusing here keeps it the one.
        if context.accessors.get(cell).is_some() {
            return refuse("the receiver has accessors of its own");
        }

        // No recorded link, no walk. This is not an optimisation: `inherited_from`
        // SUBSTITUTES a prototype by kind for arrays, callables, text and plain
        // objects, so those cells share one undiscriminated layout — and a site
        // that cached against one would recognise every other. An array and an
        // object literal holding `length` reach the same shape and the same type
        // today, which is exactly the collision this refusal removes.
        let Some(link) = context.prototype_at(cell) else {
            return refuse("the receiver has no recorded link");
        };
        let Some(holder) = crate::value::Value(link).as_slot() else {
            return refuse("the link is not a cell");
        };
        if context.proxy_at(holder).is_some() {
            return refuse("the link is a proxy");
        }
        let Ok(named_number) = u32::try_from(named.index()) else {
            return report("the key number does not fit");
        };
        if context.accessor_at(holder, named_number).is_some() {
            return refuse("the link holds an accessor for this key");
        }

        // One step, then — if the key is not there — one more. Two and no
        // further, and the bound is an argument rather than a preference: the
        // site compares three layouts, so it notices a change to the receiver,
        // to the holder, and to the cell between them. A third step would put a
        // cell in the chain that nothing compares, and relinking THAT one would
        // leave every guard satisfied and the answer stale.
        let mut middle: Option<(u32, u32)> = None;
        let mut holder = holder;
        let (_holder_type, slot) = loop {
            let Some(ty) = context.region.type_of(holder) else {
                return refuse("the link is not a cell in this region");
            };
            let Some(shape) = context.shape_of(ty) else {
                return refuse("the link has no shape");
            };
            if let Some(slot) = context.shapes.slot_of(shape, named) {
                break (ty, slot);
            }
            if middle.is_some() {
                return refuse("the key is more than two links away");
            }
            let Some(next) = context
                .prototype_at(holder)
                .and_then(|link| crate::value::Value(link).as_slot())
            else {
                return refuse("the key is absent from the link's shape");
            };
            // The cell between is walked THROUGH, so what it holds for this key
            // decides the answer as much as the holder does. A proxy or an
            // accessor on it is the same refusal it would be on either end.
            if context.proxy_at(next).is_some() || context.accessor_at(next, named_number).is_some()
            {
                return refuse("the second link is a proxy or holds an accessor");
            }
            middle = Some((holder, ty));
            holder = next;
        };
        // The HOLDER's width, since the offset is read out of the holder. A
        // prototype is an ordinary cell in the common case, so this is fifteen
        // — but a wide one is reachable by exactly the same load.
        let holder_width = context
            .region
            .width_of(holder)
            .unwrap_or(crate::heap::INLINE_SLOTS);
        if slot >= holder_width {
            return refuse("the slot is past the slots the link owns");
        }
        let Some(address) = context.region.address_of(holder) else {
            return refuse("the link has no address");
        };
        let between = match middle {
            Some((cell, _)) => match context.region.address_of(cell) {
                Some(at) => at,
                None => return refuse("the cell between has no address"),
            },
            None => 0,
        };
        // Headers rather than types, everywhere the machine compares: the
        // header carries the width beside the type and the machine compares the
        // whole word.
        let Some(receiver_header) = context.region.header_of(cell) else {
            return report("the receiver is not a cell in this region");
        };
        let Some(holder_header) = context.region.header_of(holder) else {
            return refuse("the link is not a cell in this region");
        };

        let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
            + i64::from(slot) * i64::from(rts_cranelift::mem::SLOT_BYTES);

        // SAFETY: a four-word cell this site's terminator declared, kept alive
        // for as long as the code is.
        unsafe {
            let cell = cache as *mut i64;
            cell.write(receiver_header as i64);
            cell.add(1).write(offset);
            cell.add(2).write(address as i64);
            cell.add(3).write(holder_header as i64);
            match middle {
                Some((between_cell, _)) => {
                    cell.add(4).write(between as i64);
                    cell.add(5).write(
                        context
                            .region
                            .header_of(between_cell)
                            .map_or(-1, |word| word as i64),
                    );
                }
                None => {
                    cell.add(4).write(0);
                    cell.add(5).write(-1);
                }
            }
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
