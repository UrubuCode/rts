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
    resolve(object, key, cache, Reaches::Overflow)
}

/// Whether the site asking may be answered with a property that lives OUTSIDE
/// the receiver's cell.
///
/// A read may: `lower_cached_get` loads the third word and, when it is not
/// zero, takes the block's address out of the receiver before adding the
/// offset. A WRITE may not, and that is not a policy — `lower_cached_set`
/// stores at `address + offset` and reads no third word at all, so answering a
/// store with an overflow offset writes INTO THE RECEIVER'S CELL at a position
/// that belongs to another property. That is the regression b9df2d9d shipped
/// knowingly — `claude-generator-alias-iteracao` and `node_fs_dir_readv` — and
/// the two sites are one bug because `cache_resolve_store` answers by calling
/// this resolver.
#[derive(Clone, Copy, PartialEq)]
enum Reaches {
    /// The cell only: what a store site can act on.
    Cell,
    /// The cell, or the block it keeps its overflow in.
    Overflow,
}

fn resolve(object: u64, key: i64, cache: i64, reaches: Reaches) -> i64 {
    with_current(|context| {
        context.resolves += 1;
        let explain = |why: &'static str, context: &mut Context| {
            // The census, when one was asked for. Counted before the sampled
            // line below and independently of it: the sampling answers "what
            // does a miss look like", this answers "how many, of what, where".
            if context.census.is_some() {
                let number = u32::try_from(key).unwrap_or(u32::MAX);
                let site = cache as u64;
                if let Some(census) = context.census.as_mut() {
                    *census.entry((why, number, site)).or_insert(0) += 1;
                }
            }
            // A miss is ORDINARY the first time a site sees a layout — that is
            // what a cache is. What this exists to catch is the site that
            // misses forever, so it reports the reason and the key rather than
            // a count, and stops after twenty so a real program stays readable.
            if (context.resolves <= 20 || context.resolves % 200_000 == 0) && super::switches::cache_debug() {
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
                cell.add(2).write(0);
            }
            return offset;
        }
        let Some(shape) = context.shape_of(ty) else {
            // A string, or a layout nothing recorded. Not an object, so nothing
            // is at any offset in it.
            explain("the receiver has no shape: a string, or a layout nothing recorded", context);
            return -1;
        };
        // What the receiver's shape ACTUALLY holds, for the one question the
        // census cannot answer: a site that misses forever with "absent from
        // the receiver's shape" is either reading an inherited property, or
        // reading one the object genuinely lacks, or seeing a type older than
        // the write that added it — and those want three different fixes.
        //
        // Sampled rather than printed: the first three, then one every twenty
        // thousand, so a site that misses a million times is seen late as well
        // as early. It was written for exactly that: on `{x:i}; o.y=i; a+=o.y`
        // the read reports shape ["x"] on every sample while the program answers
        // correctly, which says the read resolves against a type older than the
        // transition the write took.
        if (context.resolves <= 3 || context.resolves % 20_000 == 0) && super::switches::cache_why() {
            let held: Vec<String> = context
                .shapes
                .properties(shape)
                .into_iter()
                .map(|(k, _)| {
                    context
                        .interner
                        .text(k)
                        .and_then(|t| t.to_rust())
                        .unwrap_or_else(|| "?".to_owned())
                })
                .collect();
            eprintln!(
                "rts-why get cell {} key#{number} ty {ty} shape {shape:?} holds {held:?} slot {:?}",
                object as u32,
                context.shapes.slot_of(shape, key)
            );
        }
        // A STORE that adds a property is the dominant miss in every program that
        // builds objects in a loop, and it cannot be cached the ordinary way:
        // the layout the site would have to recognise is the one BEFORE the
        // write and the offset it would have to answer only exists after. So
        // the growth is taken HERE, and the site is answered with the offset it
        // now has — the machine's store lands correctly and the second crossing
        // (the miss path calling `set_property`) is not paid at all.
        //
        // Measured before this existed, on bench/analytic.ts: 1 419 894 misses,
        // of which 1 135 617 were one site — `new Callee(); o.v` — growing one
        // property per iteration on a fresh object.
        //
        // The representation the shape records is `Tagged` and not what the value
        // turns out to be, because this resolver is not given the value. That
        // is a shape-identity difference and NOT a correctness one: every
        // runtime reader of `properties(shape)` discards the repr, and the only
        // thing that reads it back is `typed_as`, to mint a type NUMBER. A slow
        // `put` of a number takes the F64 transition instead, so the two paths can
        // reach two shapes for one key set — which costs a miss, never a value.
        if reaches == Reaches::Cell
            && context.shapes.slot_of(shape, key).is_none()
            && !super::integrity::refuses_growth(context, object as u32)
            && context.proxy_at(object as u32).is_none()
            // A setter ANYWHERE on the chain runs instead of the property being
            // created, so growing here would silently skip it. Checking only
            // the receiver was not enough and three files said so:
            // class_extras, computed_accessor and super_field_setter.
            // An accessor with NO setter is not "no accessor": growing here
            // would put a data slot under a property the language refuses to
            // write at all, so the whole pair is asked rather than the setter.
            && super::accessor::accessor_for(context, object as u32, crate::object::Key::Name(key))
                .is_none()
            && let Ok(grown) = context
                .shapes
                .transition(shape, key, rts_cranelift::repr::Repr::Tagged)
            && let Some(at) = context.shapes.slot_of(grown, key)
            && at < context
                .region
                .width_of(object as u32)
                .unwrap_or(crate::heap::INLINE_SLOTS)
                .saturating_sub(1)
        {
            // The header BEFORE, which is what the site must recognise, and the
            // header AFTER, which is what the machine writes into the object
            // when it does. Read before the transition for the first and after
            // it for the second, because there is no other moment when both
            // exist.
            let Some(before) = context.region.header_of(object as u32) else {
                explain("the receiver is not a cell in this region", context);
                return -1;
            };
            let link = context.prototype_at(object as u32);
            let ty = context.typed_as(grown, link).index() as u32;
            context.region.set_type(object as u32, ty);
            let Some(after) = context.region.header_of(object as u32) else {
                explain("the receiver is not a cell in this region", context);
                return -1;
            };
            let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
                + i64::from(at) * i64::from(rts_cranelift::mem::SLOT_BYTES);
            // SAFETY: the cell this site declared, as everywhere else here.
            //
            // The TRANSITION is what is remembered, not the result: word zero
            // is the layout to recognise, word two the layout to write. A fresh
            // object starting at the same shape then hits — which is every
            // object built in a loop, and was a full resolve per iteration.
            unsafe {
                let cell = cache as *mut i64;
                cell.write(before as i64);
                cell.add(1).write(offset);
                cell.add(2).write(after as i64);
            }
            return offset;
        }
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
        // One short of what the cell owns: the last slot holds the address of
        // the overflow and belongs to no property.
        let width = context
            .region
            .width_of(object as u32)
            .unwrap_or(crate::heap::INLINE_SLOTS)
            .saturating_sub(1);
        if slot >= width {
            if reaches == Reaches::Cell {
                explain("the slot is past the cell and the site is a store", context);
                return -1;
            }
            // In the OVERFLOW, and reachable: the object keeps its block's
            // address in the slot it reserved for exactly this, so the machine
            // loads that word and finishes the read at the same offset. Two
            // loads instead of a call, which is what steps five and six of the
            // overflow plan were for.
            let Some((_, held)) = context.spill_of.copied(object as u32) else {
                explain("the slot is past the cell and no overflow exists yet", context);
                return -1;
            };
            let past = slot - width;
            if past >= held {
                explain("the slot is past the overflow the object has", context);
                return -1;
            }
            let offset = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
                + i64::from(past) * i64::from(rts_cranelift::mem::SLOT_BYTES);
            let through = i64::from(rts_cranelift::mem::HeaderLayout::BYTES)
                + i64::from(width) * i64::from(rts_cranelift::mem::SLOT_BYTES);
            // SAFETY: the cell this site declared, as everywhere else here.
            unsafe {
                let cell = cache as *mut i64;
                cell.write(remembered as i64);
                cell.add(1).write(offset);
                cell.add(2).write(through);
            }
            return offset;
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
            // Zero: the answer is in the cell asked about. Written rather than
            // assumed, because this site may have been resolved before against
            // an object that HAD overflowed.
            cell.add(2).write(0);
        }
        offset
    })
}

/// The census, rendered: every miss by reason, then by key, then by site.
///
/// # It counts STORES as well as reads, and that is not a rounding error
///
/// [`cache_resolve_store`] answers by calling [`cache_resolve`], so a store
/// site that misses lands here too. On `{x:i}; o.y=i; a+=o.y` that is the whole
/// number: the store of `y` resolves against an object that is still `["x"]`,
/// because a store that ADDS a property cannot find it in the shape by
/// definition, and the read that follows resolves against `["x","y"]` and
/// arms. The line the write-side probe prints immediately after
/// `get cell N … slot None` is that same iteration's transition, which is why
/// the two looked impossible to order.
///
/// So "2 271 863 misses, dominant reason: absent from the receiver's shape" is
/// not a read problem at all. It is one growth store per iteration, on a fresh
/// object each time, and no cache keyed on the receiver's CURRENT layout can
/// ever arm for it — the layout it must recognise is the one BEFORE the write
/// and the offset it must answer only exists after. What arms is a transition
/// remembered as a pair (this header + this key -> that header, that offset),
/// which is a different thing to cache and is not built here.
///
/// Three tables and not one, because they answer three different questions and
/// the first one that has an answer is the one to act on. A reason says WHAT to
/// build. A key says which property is not where the site expects it. A site
/// missing millions of times alone is polymorphic or unarmable, where a million
/// sites missing once each is just a big program — and a total cannot tell
/// those apart, which is why the total was all we had and it said nothing.
///
/// Answers `None` when no census was asked for, which is every ordinary run.
pub fn census_report(context: &Context) -> Option<String> {
    use std::fmt::Write as _;

    let census = context.census.as_ref()?;
    let total: u64 = census.values().copied().sum();

    let mut by_reason: std::collections::BTreeMap<&'static str, u64> = Default::default();
    let mut by_key: std::collections::BTreeMap<u32, u64> = Default::default();
    let mut by_site: std::collections::BTreeMap<u64, (u64, &'static str, u32)> = Default::default();
    for (&(why, key, site), &count) in census {
        *by_reason.entry(why).or_insert(0) += count;
        *by_key.entry(key).or_insert(0) += count;
        let entry = by_site.entry(site).or_insert((0, why, key));
        entry.0 += count;
    }

    let name_of = |number: u32| {
        context
            .keys
            .key(number)
            .and_then(|key| context.interner.text(key))
            .and_then(|text| text.to_rust())
            .unwrap_or_else(|| format!("#{number}"))
    };

    let mut out = String::new();
    let _ = writeln!(out, "rts-cache census: {total} misses, {} sites", by_site.len());

    let mut reasons: Vec<_> = by_reason.into_iter().collect();
    reasons.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    for (why, count) in reasons {
        let share = count as f64 * 100.0 / total.max(1) as f64;
        let _ = writeln!(out, "  {count:>10}  {share:>5.1}%  {why}");
    }

    let mut keys: Vec<_> = by_key.into_iter().collect();
    keys.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    for (key, count) in keys.into_iter().take(12) {
        let _ = writeln!(out, "  {count:>10}  key {}", name_of(key));
    }

    let mut sites: Vec<_> = by_site.into_iter().collect();
    sites.sort_by_key(|&(_, (count, _, _))| std::cmp::Reverse(count));
    for (site, (count, why, key)) in sites.into_iter().take(12) {
        let _ = writeln!(
            out,
            "  {count:>10}  site {site:#x} key {} — {why}",
            name_of(key)
        );
    }
    Some(out)
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
            if super::switches::chain_debug() {
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
            && let Some(width) = context.region.width_of(cell).map(|owned| owned.saturating_sub(1))
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
            .unwrap_or(crate::heap::INLINE_SLOTS)
            .saturating_sub(1);
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
    // NOT `cache_resolve`: a store site's lowering has no second load, so it
    // must never be told the answer is outside the cell.
    resolve(object, key, cache, Reaches::Cell)
}
