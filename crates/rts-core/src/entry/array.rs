//! Arrays: elements addressed by number rather than by name.
//!
//! # Why an array is not an object with numeric keys
//!
//! It could be, and the shape tree would refuse it: a shape is a chain of
//! transitions, one per property, so an array of a thousand elements would be a
//! thousand-deep chain and a new layout for every length a program reaches.
//! Shapes exist to make objects built the same way share a layout; a thousand
//! arrays of different lengths share nothing.
//!
//! So elements live apart from properties, in a growable store, and the cell is
//! the identity. That is the same split text already makes — a string's bytes
//! are not in the region either, because a cell is sixty-four bytes and text is
//! any length — and it is what every engine does for the same reason.
//!
//! # What an array still is
//!
//! An object. `a.x = 1` works and `a[0] = 1` does not go anywhere near the
//! shape tree, which is why [`super::objects::get_indexed`] asks *which* before
//! deciding. `length` is an ordinary property holding the count — see
//! [`set_length`] for why it is stored rather than invented.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::heap::Slot;
use crate::text::Str;
use crate::value::Value;

/// `[…]` — a new array of `length` elements, each `undefined`.
///
/// Allocated as an ordinary object, because that is what an array is: it has
/// properties and a shape like any other. Being an array is recorded beside the
/// cell rather than in place of its layout — see `Context::array_elements` for
/// why the first version, which gave arrays a reserved layout, made `a.tag = 9`
/// a silent no-op.
/// `[a, b, c, d]` — the array and every element, in ONE crossing.
///
/// Four and not N for the reason the two-field object literal gives: the
/// arguments are scalars across an `extern "C"` boundary, and a fifth would be
/// a fifth register. `count` says how many of them are real, so `[a, b]` uses
/// the same entry point with two ignored.
///
/// Measured before this existed: `[i, i, i, i]` in a loop cost 450 ns an
/// iteration, and it was FIVE crossings — one to make the array and one per
/// element — each a thread-local, a `RefCell` borrow and a bounds decision.
///
/// A hole is still a hole: this writes exactly `count` elements, so a literal
/// with one takes the older path and keeps its absent positions.
#[rtse::entry]
pub fn array_of(count: i64, v0: u64, v1: u64, v2: u64, v3: u64) -> u64 {
    with_current(|context| {
        let held = [v0, v1, v2, v3];
        let wanted = count.clamp(0, 4) as usize;
        built_in(context, held[..wanted].to_vec())
    })
}

/// `new Array(n)` and the sized store an array literal starts from.
///
/// Allocated as an ordinary object, because that is what an array is: it has
/// properties and a shape like any other.
#[rtse::entry]
pub fn array_new(length: i64) -> u64 {
    with_current(|context| {
        // BURACOS, não `undefined`: `new Array(3)` tem três posições AUSENTES,
        // e `0 in new Array(3)` é falso. O emissor também passa por aqui ao
        // montar um literal, e ali as posições escritas sobrescrevem o buraco —
        // as não escritas são justamente as que devem continuar ausentes.
        let vazio = hole_of(context);
        built_in(context, vec![vazio; length.max(0) as usize])
    })
}

/// O marcador de posição ausente. Ver [`crate::value::Singletons::hole`].
pub(in crate::entry) fn hole_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.hole),
    )
}

/// Se este word é o marcador de ausência.
///
/// Toda LEITURA de elemento passa por aqui e devolve `undefined` no lugar; quem
/// pergunta se a posição existe (`in`, `hasOwnProperty`, `Object.keys`) usa isto
/// para responder que não.
pub(in crate::entry) fn is_hole(context: &Context, held: u64) -> bool {
    held == hole_of(context)
}

/// O que uma leitura de elemento deve entregar: o valor, ou `undefined` quando
/// a posição está ausente.
///
/// # Por que uma função e não um `if` em cada site
///
/// Porque são sessenta sites, e o que vaza se um deles esquecer é um word que
/// não corresponde a nenhum valor de JavaScript — um `typeof` respondendo algo
/// impossível, ou um `===` verdadeiro entre dois buracos. Um ponto de passagem
/// é o que torna a omissão visível na revisão.
pub(in crate::entry) fn visible(context: &Context, held: u64) -> u64 {
    if is_hole(context, held) {
        undefined_of(context)
    } else {
        held
    }
}

/// The same, from a context already in hand and with its elements.
///
/// Split out because a HOST building a namespace has a `&mut Context` and no
/// ambient one — `process.argv` is an array built before the program starts —
/// and the entry point above would abort there with nothing installed.
pub(in crate::entry) fn built_in(context: &mut Context, elements: Vec<u64>) -> u64 {
    let count = elements.len();
    let store = context.arrays.insert(elements).slot();
    let cell = allocate_array_cell(context);
    context.mark_array(cell, store);
    set_length(context, cell, count);
    Value::from_slot(cell).bits()
}

/// Builds an array from a list that is still registered with the collector.
///
/// The cell is allocated FIRST, while `values` remains a [`Rooted`] list. The
/// subsequent `take`/insert/mark sequence performs no runtime allocation: the
/// values become reachable through `array_elements` before the guard is dropped.
/// This is the safe transfer for recursive materialisers such as JSON.parse.
pub(in crate::entry) fn built_in_rooted(
    context: &mut Context,
    values: super::rooted::Rooted,
) -> u64 {
    let count = values.len();
    let cell = allocate_array_cell(context);
    let store = context.arrays.insert(values.take()).slot();
    context.mark_array(cell, store);
    set_length(context, cell, count);
    Value::from_slot(cell).bits()
}

/// Allocates the array object at the shared cached layout.
fn allocate_array_cell(context: &mut Context) -> u32 {
    // Born at the layout an array ARRIVES at, rather than at the empty one and
    // then transitioning. Every array reaches the same shape — one property,
    // `length` — so the transition computed the same answer every time: a
    // shape lookup, a `transition`, a `typed_as` and a re-type, per array.
    //
    // Remembered on the context rather than recomputed, and it cannot go
    // stale: it is the layout of one shape, and a shape is immutable once
    // reached — what changes is which shape an object is AT, which is exactly
    // what the write below still does through `put` when a program grows the
    // array a property.
    let ty = match context.array_layout {
        Some(known) => known,
        None => {
            let root = context.shapes.root();
            let key = match super::computed::length_key(context) {
                crate::object::Key::Name(named) => named,
                crate::object::Key::Index(_) => {
                    // `length` is a name, always. Falling back rather than
                    // panicking keeps a malformed key registry slow instead of
                    // fatal.
                    let ty = context.layout_of(root).index() as u32;
                    return super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
                }
            };
            let Ok(grown) = context
                .shapes
                .transition(root, key, rts_cranelift::repr::Repr::F64)
            else {
                let ty = context.layout_of(root).index() as u32;
                return super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
            };
            let ty = context.layout_of(grown).index() as u32;
            context.array_layout = Some(ty);
            ty
        }
    };
    super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty)
}

impl Context {
    /// Records that a cell is an array, and where its elements are.
    fn mark_array(&mut self, cell: u32, store: Slot) {
        self.array_elements.set(cell, store);
    }

    /// Where a cell's elements are, if it is an array.
    fn store_of(&self, reference: u32) -> Option<Slot> {
        self.array_elements.copied(reference)
    }

    /// The elements of an array, if this reference names one.
    pub(super) fn elements_at(&self, reference: u32) -> Option<&Vec<u64>> {
        self.arrays.at(self.store_of(reference)?).ok()
    }

    /// The same, to write through.
    pub(super) fn elements_at_mut(&mut self, reference: u32) -> Option<&mut Vec<u64>> {
        let store = self.store_of(reference)?;
        self.arrays.at_mut(store).ok()
    }
}

/// The element a value names, if it is a canonical index in range.
///
/// # Why a whole non-negative double and not "a number"
///
/// `a[1.5]` and `a[-1]` are ordinary **properties**, not elements — the
/// language says an array index is a canonical non-negative integer below
/// 2^32-1, and everything else is a name. Getting this wrong makes `a[1.5] = 9`
/// write into element 1, which is a wrong program that runs.
pub(super) fn as_index(context: &Context, key: Value) -> Option<usize> {
    // A STRING that spells a canonical index is one too, and this is not a
    // nicety: `for (k in a)` yields strings, so `a[k]` inside such a loop is
    // always the string form. The first version took numbers only, and
    // `for (k in [1,2,3]) s += a[k]` answered NaN — every read missed the
    // elements and found an absent property.
    //
    // `as_array_index` is the runtime's own answer to which strings those are,
    // and reusing it is what keeps this agreeing with `Key::from_str` about
    // where the boundary is.
    if let Some(slot) = key.as_slot()
        && let Some(text) = context.text_at(slot)
    {
        return crate::object::as_array_index(text).map(|index| index as usize);
    }

    let number = key.numeric()?;
    if number < 0.0 || number.fract() != 0.0 || number >= 4_294_967_295.0 {
        return None;
    }
    Some(number as usize)
}

/// `for (k in o)` — the keys, as an array of strings.
///
/// # Why an array and not an iterator
///
/// Because an iterator is a call, and a call from inside an entry point is what
/// this layer cannot do. Handing back an array lets the compiler emit an
/// ordinary indexed loop, which is machinery that already exists and is already
/// tested.
///
/// # What the order is
///
/// Integer indices first in numeric order, then the other keys in the order
/// they were added. That is what the specification says and what `Key` was
/// split into two variants to record — see [`crate::object::key`]. An array's
/// elements come first for the same reason: they are the integer indices.
///
/// # The divergence, named
///
/// The keys are collected **once**, so a property deleted during the loop is
/// still visited, where the specification says it must not be. Properties
/// *added* during one need not be visited, which snapshotting also satisfies —
/// so the error is in one direction only, and it is the direction a program
/// notices least. Fixing it needs the enumeration to hold a cursor into the
/// shape rather than a copy, which is a different mechanism.
///
/// # What is missing
///
/// Inherited keys. `for-in` walks the prototype chain and there are no
/// prototypes, so this is own keys — which is the same absence every property
/// operation here has.
#[rtse::entry]
pub fn own_keys(object: u64) -> u64 {
    // A proxy answers by running its handler, and it has no own keys of its
    // own to walk — see `super::proxy` for why the interception is here rather
    // than in anything the compiled site does.
    //
    // The ENUMERABLE spelling, which is what this function is. A trap may name
    // a key the target does not have, and a key with no descriptor is not
    // enumerable — so `Object.keys` over `ownKeys: () => ["only"]` is empty,
    // and asking `proxy::own_keys` here answered `["only"]`. `proxy::keys`'s
    // own documentation says the fix is one level up, where the two spellings
    // already differ; this is that level. `own_names` keeps the unfiltered
    // form, which is what `getOwnPropertyNames` means.
    if let Some(answered) = super::proxy::enumerable_keys(object) {
        return answered;
    }
    keys_of(object, true)
}

/// The keys a `for`-`in` visits: enumerable, along the PROTOTYPE CHAIN.
///
/// # Why this is not `own_keys` with a loop at the call site
///
/// Shadowing. A key seen nearer the object is not offered again from further
/// out, **and that holds even when the nearer one is not enumerable** — a
/// non-enumerable own property hides an enumerable inherited one rather than
/// letting it through. A caller stitching `own_keys` up the chain would have
/// only the enumerable half of each level and could not express that, so it
/// would report a key the language says is hidden.
///
/// So each level is read TWICE: the enumerable keys to offer, and every own
/// name to record as seen. Both walks already exist here; what is new is
/// using them together.
///
/// # The divergence it inherits, unchanged
///
/// Collected once, so a property deleted during the loop is still visited —
/// the same snapshot [`own_keys`] documents, for the same reason, and fixing
/// it is the same different mechanism.
#[rtse::entry]
pub fn enumerate_keys(object: u64) -> u64 {
    let mut offered: Vec<crate::object::Key> = Vec::new();
    let mut seen: std::collections::BTreeSet<crate::object::Key> = std::collections::BTreeSet::new();
    let mut walking = object;
    // A bound rather than a `while let`: a prototype chain a program built with
    // `Object.setPrototypeOf` can be a CYCLE, and the language throws when one
    // is created rather than when it is walked. This engine does not check on
    // creation, so walking one here would hang — which the honesty floor calls
    // out by name. A hundred is past anything a real chain reaches.
    for _ in 0..100 {
        let Some(cell) = Value(walking).as_slot() else {
            break;
        };
        let (enumerable, every) = with_current(|context| {
            let enumerable = key_list(context, walking, true);
            let every = key_list(context, walking, false);
            // CANONICAL for the shadowing test, and this is not tidiness.
            //
            // The walk used to compare spellings, so an element at position 0
            // and an inherited property named `"0"` were one key and shadowing
            // worked. As keys they are `Index(0)` and `Name(…)`, which are not
            // equal — so the test has to put a name that SPELLS an index back
            // into the index form, or a `for`-`in` over an array whose prototype
            // carries `"0"` would offer the position twice.
            let canonical = |context: &Context, key: crate::object::Key| match key {
                crate::object::Key::Name(named) => context
                    .interner
                    .text(named)
                    .and_then(index_spelling)
                    .map_or(key, crate::object::Key::Index),
                crate::object::Key::Index(_) => key,
            };
            let enumerable: Vec<_> = enumerable
                .into_iter()
                .map(|key| (canonical(context, key), key))
                .collect();
            let every: Vec<_> = every
                .into_iter()
                .map(|key| canonical(context, key))
                .collect();
            (enumerable, every)
        });
        for (canonical, key) in enumerable {
            if seen.contains(&canonical) {
                continue;
            }
            offered.push(key);
        }
        seen.extend(every);
        let _ = cell;
        let next = super::chain::get_prototype(walking);
        if next == walking {
            break;
        }
        walking = next;
    }
    interned(offered)
}

/// Every own key, INCLUDING the ones an enumeration does not report.
///
/// What `Object.getOwnPropertyNames` answers, and the reason the two are one
/// function with a flag rather than two walks: they differ in a single `if`,
/// and the ordering, the symbol rule and the accessor pass are the same rules
/// — which this crate keeps refusing to state twice.
pub(in crate::entry) fn own_names(object: u64) -> u64 {
    // A proxy answers both spellings the same way — its handler's `ownKeys` IS
    // `[[OwnPropertyKeys]]`, and the enumerable/every distinction is applied by
    // the caller rather than by the trap. Asking here as well as in `own_keys`
    // is what stopped `Reflect.ownKeys` over a proxy from answering the empty
    // list, which is what a cell with no properties of its own really has.
    if let Some(answered) = super::proxy::own_keys(object) {
        return answered;
    }
    keys_of(object, false)
}

/// The shared walk, from a context already in hand.
///
/// Split out from [`keys_of`] because a host walking a value's structure holds a
/// context by construction — `entry::member_names` is that caller — and the
/// ambient form would be a nested borrow, which in an `extern "C"` frame is an
/// abort rather than an error.
/// The same walk, answering KEYS.
///
/// # Why the keys and not their text
///
/// A key is a number the registry issued; its text is a heap buffer beside that
/// number. This walk used to answer the text, so every enumeration cloned one
/// `Str` per key — `Object.keys` of a four-property object built four strings
/// that already existed, on every call — and `interned` then hashed each clone
/// back to the number it started from.
///
/// The numbers are what a shape holds and what `key_value` is keyed by, so they
/// are what this answers. [`key_texts`] is the wrapper, for the callers that
/// genuinely want text.
pub(in crate::entry) fn key_list(
    context: &mut Context,
    object: u64,
    enumerable_only: bool,
) -> Vec<crate::object::Key> {
    {
        let Some(slot) = Value(object).as_slot() else {
            return Vec::new();
        };

        // A primitive string is a heap `Reference` like an object, and until
        // this branch existed it fell straight through to `region.type_of`,
        // which answers `None` for it — so `Object.keys("ab")` answered `[]`
        // rather than the per-character indices the language says a string
        // has. Answered here and returned immediately: a string has no shape
        // and no accessors, so there is nothing further in this function for
        // it to reach.
        // A BOXED string reaches the same answer through one more link, and it
        // has to: `new String("ab")` and `Object("ab")` are objects whose text
        // hangs off `boxed_at` rather than being the cell, so reading only
        // `text_at` answered `[]` for them. That went unnoticed while a wrapper
        // WAS its primitive — the moment one became a real object,
        // `Object.keys(Object("ab"))` stopped naming its characters.
        let held = context
            .text_at(slot)
            .is_none()
            .then(|| Value(context.boxed_at(slot)?).as_slot())
            .flatten();
        if let Some(text) = held.map_or_else(|| context.text_at(slot), |cell| context.text_at(cell))
        {
            let mut keys: Vec<crate::object::Key> = (0..text.units().count())
                .map(|index| crate::object::Key::Index(index as u32))
                .collect();
            // `"length"` too, but only for `getOwnPropertyNames`: a boxed
            // string has an own, non-enumerable `length`, the same as an
            // array's, and `Object.keys`/`for`-`in` are right to skip it —
            // `getOwnPropertyNames` is not, and used to stop at the indices,
            // so `new String("hi")`'s own names answered `["0","1"]` and
            // never the one every other own-name walk here already gives an
            // array.
            if !enumerable_only {
                keys.push(super::computed::length_key(context));
            }
            return keys;
        }

        let mut keys: Vec<crate::object::Key> = Vec::new();
        // Elements first, as strings: `for (k in [1,2])` yields "0" and "1",
        // not 0 and 1. A loop that compared `k === 0` would find nothing, and
        // that is the language rather than a quirk of this implementation.
        if let Some(elements) = context.elements_at(slot) {
            // Um BURACO não tem chave. `Object.keys([,1])` é `["1"]`, e daqui
            // saem também `for-in`, `Object.values/entries`, `Object.assign`,
            // `structuredClone` e o `JSON.stringify` de objeto — todos corretos
            // por causa deste `continue`.
            //
            // Filter in place. Building `ausentes` first used a second heap
            // allocation and a second full pass over the element store even
            // though the result only needs the non-hole indices.
            keys.reserve(elements.len());
            for (index, &held) in elements.iter().enumerate() {
                if !is_hole(context, held) {
                    keys.push(crate::object::Key::Index(index as u32));
                }
            }
        }
        let Some(ty) = context.region.type_of(slot) else {
            return keys;
        };
        let Some(shape) = context.shape_of(ty) else {
            return keys;
        };
        // The two sequences are merged before either is filtered, because
        // enumeration order is INSERTION order and an accessor is deliberately
        // not in the layout. Each accessor carries how many properties the
        // shape held when it was defined, so putting it back in front of that
        // property restores the order the program wrote — `{ get a(){}, b: 1 }`
        // enumerates `a, b`, and used to enumerate `b, a` because accessors were
        // appended after the whole layout.
        //
        // Merged first and filtered after: a non-enumerable property still
        // occupies its place in the sequence, so dropping it early would move
        // everything an accessor is ranked against.
        let ranked = context.ranked_accessors(slot);
        let mut order: Vec<rts_cranelift::shape::Key> = Vec::new();
        let mut next = 0;
        for (position, (key, _)) in context.shapes.properties(shape).into_iter().enumerate() {
            while next < ranked.len() && (ranked[next].1 as usize) <= position {
                order.push(ranked[next].0);
                next += 1;
            }
            order.push(key);
        }
        while next < ranked.len() {
            order.push(ranked[next].0);
            next += 1;
        }

        for key in order {
            // An array's `length` and a collection's `size` are real properties
            // — so that compiled code and the runtime answer them the same way
            // — and the language says neither is enumerable. That used to be a
            // special case here naming `length`; it is now the ordinary
            // attribute every property has, recorded where each is written.
            //
            // Caught by a probe when it was missing: `for (k in [1,2,3])`
            // summed to 9 instead of 6, because the loop visited "length".
            if enumerable_only && !super::integrity::enumerable(context, slot, key) {
                continue;
            }
            if let Some(text) = context.interner.text(key) {
                // A symbol-keyed property is not enumerated. Its key lives in a
                // reserved name space rather than in a third `Key` variant —
                // see [`super::symbol`] for why — so this is the one place the
                // encoding has to be known, and it is the whole cost of that
                // decision.
                if super::symbol::is_symbol_key(text) {
                    continue;
                }
                keys.push(crate::object::Key::Name(key));
            }
        }
        ordered_keys(context, keys)
    }
}

/// The symbol-keyed enumerable own properties `key_texts` deliberately drops.
///
/// `Object.keys`/`for`-`in` are right to skip a symbol key — it has no string
/// spelling for either to report — but `Object.assign` and object-spread copy
/// **every** enumerable own property, symbols included, and used to answer an
/// object merge that silently dropped a `[sym]: v` entry. That is data loss
/// dressed as success, the shape this crate's honesty rules single out as
/// worse than a refusal.
///
/// Answered as `(machine key, value)` rather than routed back through a JS
/// value and `get_indexed`: a symbol has no value this engine can name outside
/// its own key encoding (see [`super::symbol`]), so reading the value directly
/// off the shape is the only path there is.
pub(in crate::entry) fn symbol_keyed(context: &mut Context, object: u64) -> Vec<(crate::object::Key, u64)> {
    symbol_keyed_with(context, object, true)
}

/// The same walk, every own symbol key rather than only the enumerable ones —
/// what `Object.getOwnPropertySymbols` needs, for the reason
/// `getOwnPropertyNames` differs from `Object.keys`: enumerability is a filter
/// on top of ownership, not part of what "has this key" means.
pub(in crate::entry) fn symbol_keyed_with(
    context: &mut Context,
    object: u64,
    enumerable_only: bool,
) -> Vec<(crate::object::Key, u64)> {
    let Some(slot) = Value(object).as_slot() else {
        return Vec::new();
    };
    let Some(ty) = context.region.type_of(slot) else {
        return Vec::new();
    };
    let Some(shape) = context.shape_of(ty) else {
        return Vec::new();
    };
    let keys: Vec<rts_cranelift::shape::Key> = context
        .shapes
        .properties(shape)
        .into_iter()
        .filter(|(key, _)| !enumerable_only || super::integrity::enumerable(context, slot, *key))
        .filter(|(key, _)| {
            context
                .interner
                .text(*key)
                .is_some_and(super::symbol::is_symbol_key)
        })
        .map(|(key, _)| key)
        .collect();
    keys.into_iter()
        .filter_map(|key| {
            let value = super::objects::read_property(context, slot, crate::object::Key::Name(key))?;
            Some((crate::object::Key::Name(key), value.bits()))
        })
        .collect()
}

/// The shared walk.
fn keys_of(object: u64, enumerable_only: bool) -> u64 {
    interned(with_current(|context| {
        key_list(context, object, enumerable_only)
    }))
}

/// A list of key texts, as a JavaScript array of interned strings.
///
/// Split out of [`keys_of`] when [`enumerate_keys`] needed the same tail with a
/// different head — a walk UP the chain rather than one level — and the
/// alternative was a second copy of the in-place write below, which is where
/// one of the two would come to allocate twice again.
fn interned(keys: Vec<crate::object::Key>) -> u64 {
    // Built outside the borrow above, because interning each string and
    // allocating the array both need the context again.
    let array = array_new(keys.len() as i64);
    // Written STRAIGHT INTO the array, one key at a time.
    //
    // It used to intern into a second `Vec` and then replace the array's own —
    // so `array_new` sized a vector of holes that was thrown away, and every
    // call allocated twice for one list.
    //
    // Writing in place also removes the reason this needed `super::rooted`:
    // interning allocates and an allocation collects, but a key that has landed
    // in the array is reachable THROUGH it, and the array is named by a local
    // of this frame that the stack scan does see. Nothing is ever held only by
    // a `Vec` the collector cannot reach.
    with_current(|context| {
        let Some(slot) = Value(array).as_slot() else {
            return;
        };
        for (at, key) in keys.into_iter().enumerate() {
            // Through the key cache, because these ARE keys: every one came
            // from a shape or from an element index, both closed sets the
            // interner already holds. `intern_value` allocated a fresh cell per
            // key per call — `Object.keys` of a four-property object built four
            // strings that already existed, every time it was asked.
            //
            // By NUMBER now, which is the whole of the second round: `key_value`
            // is keyed by the number the walk already had, where `key_text_value`
            // took the text and hashed it back to that number. A named key
            // therefore costs one lookup in a cache `roots` already scans, and no
            // `Str` is cloned anywhere on this path.
            //
            // An element position is the exception and has to be: its spelling
            // is a NUMBER's, which no shape holds, so it is built here — which
            // is what `Object.keys` of an array pays and an object does not.
            let value = match key {
                crate::object::Key::Name(named) => context.key_value(named),
                crate::object::Key::Index(number) => {
                    context.key_text_value(&crate::coerce::number_to_string(f64::from(number)))
                }
            };
            if let Some(elements) = context.elements_at_mut(slot) {
                elements[at] = value;
            }
        }
    });
    array
}

/// An element of an array the CALLER has already proved is one, at an index it
/// has already proved is a canonical integer in range.
///
/// # What it does not ask, and why it may not ask it
///
/// `get_indexed` answers `o[k]` for any receiver and any key, and every
/// question it asks is one the caller could not have answered: is the receiver
/// a proxy, is the key a canonical index, is the receiver a typed array, a
/// string, or an object with a property under that name.
///
/// The one caller here is `rts-codegen`'s `for-of` desugaring, and it knows all
/// of them by CONSTRUCTION. The array is the copy `iterate` just answered — a
/// fresh vector no program can name, so it is not a proxy, not a view and not a
/// string — and the index is the counter the compiler minted, starting at zero
/// and stopping at the length it read off that same array. There is nothing
/// left to establish.
///
/// This is the whole of what a compiler that owns the program can do that one
/// guessing cannot: the proof happens once, while compiling, instead of per
/// element, forever.
///
/// # What it still does
///
/// Maps a hole to `undefined`, because `iterate` copies elements verbatim and
/// `for (const x of [1, , 3])` yields three values of which the middle is
/// `undefined`. And answers `undefined` for anything it cannot reach, rather
/// than trusting the caller further than the code can check: a receiver that is
/// somehow not an array, or an index somehow past the end, gets the answer an
/// absent element has instead of a read of whatever was there.
#[rtse::entry]
pub fn element_at(array: u64, index: u64) -> u64 {
    with_current(|context| {
        let at = Value(index).numeric().unwrap_or(-1.0);
        let Some(cell) = Value(array).as_slot() else {
            return undefined_of(context);
        };
        let Some(elements) = context.elements_at(cell) else {
            return undefined_of(context);
        };
        let Some(held) = elements.get(at as usize).copied() else {
            return undefined_of(context);
        };
        visible(context, held)
    })
}

/// Where an array's elements START, as a machine address.
///
/// # What the caller is taking on
///
/// That the run does not MOVE while it holds this. The elements are a `Vec`,
/// and pushing to one reallocates — so an address handed out here is good only
/// for as long as nothing grows that array.
///
/// The one caller is `rts-codegen`'s `for-of` desugaring, and it is safe there
/// for a reason nothing else can borrow: the array is the copy `iterate` just
/// made, no program can name it, and the loop only reads. `iterate` copies
/// deliberately — its own documentation says a body that pushes to the original
/// must not walk its own additions — and that same copy is what makes the
/// address stable.
///
/// # The bound is established beside the address
///
/// `foreach.rs` asks the internal array's length through `ArrayLength`, which
/// returns F64 rather than a user-visible property value. That makes the
/// existing `ElementLoad` producer reachable for non-suspending loops while
/// preserving the safe [`element_at`] fallback for resumable bodies.
///
/// **Do not price that gap by differencing a `for-of` that reads its binding
/// against one that ignores it.** The binding is pushed unconditionally, so the
/// call is in both arms and cancels; the difference measures an unbox, not a
/// load.
///
/// Answers `0` for anything that is not an array, which the caller must treat
/// as "no run": zero elements, so a bounded read of it is refused by its own
/// bound before the address is ever used.
#[rtse::entry]
pub fn elements_base(array: u64) -> i64 {
    with_current(|context| {
        let Some(cell) = Value(array).as_slot() else {
            return 0;
        };
        match context.elements_at(cell) {
            Some(elements) => elements.as_ptr() as i64,
            None => 0,
        }
    })
}

/// The length of an internal array returned by a compiler-owned enumeration.
///
/// This is narrower than a JavaScript `length` property read: the only caller is
/// the `for-of`/`for-in` lowering after it has obtained a fresh array from an
/// enumeration entry. The result is an unboxed double, so the loop header can
/// feed the machine's bounded element load without a tagged property crossing.
#[rtse::entry]
pub fn array_length(array: u64) -> f64 {
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .map_or(0.0, |elements| elements.len() as f64)
    })
}

/// Writes an array's `length` as an ordinary property.
///
/// # Why a real property and not an answer the runtime invents
///
/// It WAS invented: `get_property` special-cased the key and answered the
/// element count. That worked until something stored a `length` property, and
/// then the two paths disagreed — because compiled code does not go through
/// `get_property` for a hit. It emits `cached_get`, which finds the stored
/// property and never asks the runtime at all.
///
/// A special case only the slow path knows about is a special case that stops
/// applying the moment the fast path starts working, which is the opposite of
/// how a fast path is supposed to fail. So the count is a property both read.
///
/// The divergence that remains, named: assigning `length` stores a number and
/// does not truncate the array, where the language shortens it. Truncating
/// needs the write to know it is writing to an array, which is `put`'s caller
/// rather than `put`.
pub(super) fn set_length(context: &mut Context, cell: u32, length: usize) {
    let key = super::computed::length_key(context);
    let value = Value::from_f64(length as f64).bits();
    // Ordinary arrays already own `length` in their current shape. Updating an
    // existing slot cannot change the shape, so avoid the generic property path
    // on every push/pop; first materialisation and malformed layouts keep `put`.
    if let crate::object::Key::Name(name) = key
        && let Some(ty) = context.region.type_of(cell)
        && let Some(shape) = context.shape_of(ty)
        && let Some(slot) = context.shapes.slot_of(shape, name)
    {
        // `length` is born in the array layout, so the slot exists before the
        // first call reaches this branch. Its descriptor does not: the shape
        // stores only the value representation, while array-specific attributes
        // live beside the cell. Initialise that side table once, then preserve
        // any descriptor subsequently installed by `defineProperty`, freeze, or
        // seal.
        if !context.has_attributes(cell, name) {
            super::integrity::set_attributes(
                context,
                cell,
                name,
                super::integrity::Attributes {
                    writable: true,
                    enumerable: false,
                    configurable: false,
                },
            );
        }
        // The raw slot write deliberately has no descriptor check; do it before
        // using the fast path, otherwise push/pop could change a frozen array
        // while its `length` property stayed unchanged.
        if super::integrity::refuses_key_write(context, cell, name) {
            return;
        }
        super::objects::set_slot_value(context, cell, slot, value);
        return;
    }
    super::objects::put(context, cell, key, value);
    // The attributes are NOT written here, and used to be: `{writable,
    // !enumerable, !configurable}` was recorded against this cell and this key
    // on every array built and every `push`. It said the same three flags every
    // time, so `Context::implied_attributes` answers them from the fact the
    // cell already carries — that `array_elements` names it — and this function
    // is one `put`.
    //
    // What it cost measured 2026-08-26, ablating the two halves separately in
    // one binary: of a 136.6 ns `[]`, the `put` was 24 ns and the record was
    // **84** — the first attribute of a cell allocates a `Vec` and grows an
    // `Aside`. `[]` is 52.4 ns without it, under `new C()` at 76.
    //
    // The read-back this used to do (`writable` from `refuses_key_write`, so a
    // `push` could not undo a program's `{writable: false}`) went with it, and
    // for a stronger reason than it was written for: a recorded deviation is
    // now the only record there is, so nothing overwrites it.
}

/// Whether a key's spelling IS the canonical one for an array index.
///
/// The same question [`ordered_keys`] asks, answered without allocating. The old
/// spelling was `text.to_rust()?` — a `String` on the heap — then `parse`, then
/// `number.to_string()` for the round trip: **two allocations per key per
/// enumeration**, to decide something that is fixed for the life of the key.
/// `Object.keys` of a four-property object paid it four times, every call.
///
/// The round trip is preserved exactly, and it is what makes `"01"` and `"1.0"`
/// ordinary names as the specification says. Reading it as digits gives the same
/// answer: a leading zero is only canonical for `"0"` itself, anything that is
/// not a digit is not an index, and an overflow past `u32::MAX` is not one
/// either — which also covers `u32::MAX`, the one value the language reserves
/// for `length` rather than for an element.
fn index_spelling(text: &Str) -> Option<u32> {
    let length = text.len();
    if length == 0 || length > 10 {
        return None;
    }
    let first = text.unit_at(0)?;
    if first == b'0' as u16 {
        return (length == 1).then_some(0);
    }
    let mut number: u32 = 0;
    for at in 0..length {
        let unit = text.unit_at(at)?;
        if !(b'0' as u16..=b'9' as u16).contains(&unit) {
            return None;
        }
        number = number.checked_mul(10)?.checked_add(u32::from(unit - b'0' as u16))?;
    }
    (number != u32::MAX).then_some(number)
}

/// The enumeration order the language states, over keys already collected.
///
/// **Array-index keys first, in ascending numeric order; then everything else
/// in the order it was added.** Not insertion order overall, and the difference
/// is not exotic: `o.b = 1; o[2] = 1; o.a = 1; o[1] = 1` enumerates
/// `1, 2, b, a`.
///
/// # Why a `Key::Name` still has to be asked
///
/// [`super::computed`] turns every computed key into a NAME, including one that
/// spells an index, and its own documentation says why: `o[0]` and `o["0"]` are
/// one property, and routing an object's key through `Key::from_str` made
/// `o[0] = 1; o[0]` read as absent. What that gave up was exactly this ordering,
/// recorded there as "an enumeration order nothing implements".
///
/// So a `Key::Index` — which only an element position produces — is already the
/// number this sorts by, and a `Key::Name` has its spelling asked instead. A key
/// is an index when that spelling is the canonical one for a number below
/// 2^32 − 1, which is what makes `"01"` and `"1.0"` ordinary names as the
/// specification says, and what a `parse` alone would have got wrong.
///
/// The text is READ, never cloned: [`index_spelling`] walks its units.
fn ordered_keys(context: &Context, keys: Vec<crate::object::Key>) -> Vec<crate::object::Key> {
    let index_of = |key: &crate::object::Key| -> Option<u32> {
        match key {
            crate::object::Key::Index(number) => Some(*number),
            crate::object::Key::Name(named) => index_spelling(context.interner.text(*named)?),
        }
    };

    let mut indices: Vec<(u32, crate::object::Key)> = Vec::new();
    let mut names: Vec<crate::object::Key> = Vec::new();
    for key in keys {
        match index_of(&key) {
            Some(number) => indices.push((number, key)),
            None => names.push(key),
        }
    }
    indices.sort_by_key(|(number, _)| *number);
    let mut out: Vec<crate::object::Key> = indices.into_iter().map(|(_, key)| key).collect();
    out.extend(names);
    out
}

/// The keys a walk answers, as TEXT.
///
/// The wrapper over [`key_list`] for the callers that want spellings rather than
/// numbers — a `for`-`in` binding, a proxy invariant check, a module's member
/// names. Each pays one `Str` per key, which is what the walk itself used to
/// charge every caller including the ones that only wanted the number back.
pub(in crate::entry) fn key_texts(
    context: &mut Context,
    object: u64,
    enumerable_only: bool,
) -> Vec<Str> {
    key_list(context, object, enumerable_only)
        .into_iter()
        .filter_map(|key| match key {
            crate::object::Key::Index(number) => {
                Some(crate::coerce::number_to_string(f64::from(number)))
            }
            crate::object::Key::Name(named) => context.interner.text(named).cloned(),
        })
        .collect()
}

