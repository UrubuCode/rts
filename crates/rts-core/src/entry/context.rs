//! What the context answers about itself.
//!
//! # Why this is beside the struct rather than in it
//!
//! `mod.rs` declares what a running program's operations need, and the fields
//! carry a page of reasoning each — which is the file's job. These are the
//! questions asked *of* those fields, and keeping them here is what let the
//! declaration stay readable when the eighth side table arrived.
//!
//! A child module rather than a sibling, and that is load-bearing: a private
//! field is visible to the module that declares it and to its descendants, so
//! `entry::context` reaches them and a crate-level `context` module would not.

use super::Context;
use crate::heap::Slot;
use crate::text::Str;
use crate::value::Value;

/// The layout a callable of one shape arrives at, and where its own properties
/// sit in it.
///
/// Three slots in a fixed order — `prototype`, `name`, `length` — each `None`
/// when the shape this describes does not carry that property. See
/// [`Context::callable_template`] for why this is recorded rather than computed.
#[derive(Clone, Copy)]
pub(super) struct CallableTemplate {
    /// The type number a cell of this shape is born at, so a closure can be
    /// allocated already carrying it instead of transitioning into it.
    pub(super) ty: u32,
    /// Where `prototype`, `name` and `length` land, in that order.
    pub(super) slots: [Option<u32>; 3],
}

impl Context {
    /// The layout a shape arrives at, remembering the way back.
    ///
    /// `ShapeTree::layout` is what turns a shape into an aggregate. The reverse
    /// is recorded here because a cell's header holds the TYPE and a property
    /// lookup needs the SHAPE — and searching every layout per access is the
    /// cost this design exists to remove.
    pub fn layout_of(
        &mut self,
        shape: rts_cranelift::shape::ShapeId,
    ) -> rts_cranelift::types::TypeId {
        let ty = self.shapes.layout(shape, &mut self.types);
        self.record_shape(ty, shape);
        ty
    }

    /// The layout a shape arrives at, distinguished by what the cell inherits
    /// from.
    ///
    /// # Why this exists beside [`Self::layout_of`]
    ///
    /// Because a shape says which properties an object holds and says nothing
    /// about what it inherits — correctly, since inheritance is not a layout.
    /// But an inline cache that may answer out of an inherited cell has to
    /// distinguish two objects that hold the same fields and inherit from
    /// different places, and the only thing it compares is the type number.
    ///
    /// So the discrimination goes in the NUMBER, over one unchanged shape,
    /// exactly as `integrity::retype` already does for a frozen cell. The shape
    /// tree is untouched: it would have been the wrong home for this, because
    /// `ShapeTree::remove` and `redefine` rebuild a shape from the root, so a
    /// discriminator recorded there would be silently dropped by `delete o.x` —
    /// and dropped toward *the same shape*, which merges two layouts and makes
    /// the cache HIT where it must miss. Every other part of shape identity
    /// fails toward "different"; that one would have failed toward "same".
    ///
    /// # Why a cell with no link keeps the shared layout
    ///
    /// A cell with nothing recorded gets [`Self::layout_of`] unchanged, and that
    /// is required rather than a shortcut: `objects::inherited_from` SUBSTITUTES
    /// a prototype by kind for arrays, callables, text and plain objects, so
    /// those cells record no link and must go on sharing one layout. It is also
    /// what makes the resolver's refusal of a link-less receiver sound.
    pub(super) fn typed_as(
        &mut self,
        shape: rts_cranelift::shape::ShapeId,
        link: Option<u64>,
    ) -> rts_cranelift::types::TypeId {
        let Some(cell) = link.and_then(|value| crate::value::Value(value).as_slot()) else {
            return self.layout_of(shape);
        };

        if let Some(known) = self.proto_types.get(cell)
            && let Some((_, ty)) = known.iter().find(|(at, _)| *at == shape)
        {
            return *ty;
        }

        // `types.declare` rather than `shapes.layout`: the latter memoises one
        // number per shape, so asking it here would hand back the very number
        // this call exists to differ from. Declaring is what mints a fresh one,
        // and it is the same call `integrity::retype` makes for the same reason.
        let fields: Vec<_> = self
            .shapes
            .properties(shape)
            .into_iter()
            .map(|(_, repr)| repr)
            .collect();
        let fresh = self.types.declare(&fields);
        self.record_shape(fresh, shape);
        match self.proto_types.get_mut(cell) {
            Some(known) => known.push((shape, fresh)),
            None => self.proto_types.set(cell, vec![(shape, fresh)]),
        }
        fresh
    }

    /// Records which shape a layout came from.
    ///
    /// Split out of [`Self::layout_of`] because a second caller appeared:
    /// `integrity::retype` mints a DUPLICATE layout for one cell and points it
    /// at the shape that cell already had. Both need the reverse map filled the
    /// same way, and the resize-then-write pair is exactly the kind of thing
    /// that gets written differently the second time.
    pub(super) fn record_shape(
        &mut self,
        ty: rts_cranelift::types::TypeId,
        shape: rts_cranelift::shape::ShapeId,
    ) {
        if self.shape_of_type.len() <= ty.index() {
            self.shape_of_type.resize(ty.index() + 1, shape);
        }
        self.shape_of_type[ty.index()] = shape;
    }

    /// Which shape a cell's type came from, if it is an object's.
    ///
    /// `None` for a string's layout and for a callable's, which is what makes a
    /// reference's kind readable from the object rather than from the encoding
    /// — the machine's own answer to a tag space that has no room for one.
    ///
    /// # Why both reserved layouts have to be named here
    ///
    /// Because `shape_of_type` is grown with `resize(index + 1, shape)`, which
    /// fills every new position with the shape being recorded. So the moment an
    /// object's layout is numbered above a reserved one, the reserved one's
    /// position holds a real shape that was never its own — and a callable
    /// would answer property reads by interpreting its code address as a field.
    ///
    /// Excluding by index rather than fixing the fill: the reserved layouts are
    /// the two positions that legitimately have no shape, and saying so is the
    /// fact, where a sentinel fill would be a way of encoding it.
    /// The number the text layout was given.
    ///
    /// Exposed so the cache can recognise a string without asking `shape_of`,
    /// which answers `None` for it by design.
    pub fn text_type_index(&self) -> u32 {
        self.text_type.index() as u32
    }

    /// Which shape a cell's type came from, if it is an object's.
    ///
    /// `None` for a string's layout and for a callable's, which is what makes a
    /// reference's kind readable from the object rather than from the encoding.
    /// The exclusion is load-bearing: `shape_of_type` grows with `resize(index +
    /// 1, shape)`, so a reserved position would otherwise hold a shape that was
    /// never its own and a callable would answer property reads by interpreting
    /// its code address as a field.
    pub fn shape_of(&self, ty: u32) -> Option<rts_cranelift::shape::ShapeId> {
        let ty = ty as usize;
        if ty == self.text_type.index() {
            return None;
        }
        self.shape_of_type.get(ty).copied()
    }

    /// What a cell calls, if it is callable.
    ///
    /// A method rather than a public field so nothing outside this module can
    /// claim a cell is callable, which is what makes the code address
    /// unreachable from anything a program can write.
    pub(super) fn callable_at(&self, cell: u32) -> Option<(u64, u64)> {
        self.callables.copied(cell).map(|(code, env, _)| (code, env))
    }

    /// The list an iterator walks, and how far it has gone.
    pub(super) fn cursor_at(&self, cell: u32) -> Option<(u64, u32)> {
        self.cursors.copied(cell)
    }

    /// Positions an iterator over a list.
    pub(super) fn set_cursor(&mut self, cell: u32, listed: u64, at: u32) {
        self.cursors.set(cell, (listed, at));
    }

    /// What a proxy stands for: its target and its handler.
    pub(super) fn proxy_at(&self, cell: u32) -> Option<(u64, u64)> {
        self.proxies.copied(cell)
    }

    /// Records what a proxy stands for.
    pub(super) fn set_proxy(&mut self, cell: u32, target: u64, handler: u64) {
        self.proxies.set(cell, (target, handler));
    }

    /// What a cell inherits from, if anything.
    pub(super) fn prototype_at(&self, cell: u32) -> Option<u64> {
        self.prototypes.copied(cell)
    }

    /// Sets what a cell inherits from.
    pub(super) fn set_prototype(&mut self, cell: u32, prototype: u64) {
        self.prototypes.set(cell, prototype);
    }

    /// Whether a callable asks its parent for the object it builds.
    pub(super) fn is_derived(&self, cell: u32) -> bool {
        self.derived.copied(cell).unwrap_or(false)
    }

    /// Records that it does.
    pub(super) fn mark_derived(&mut self, cell: u32) {
        self.derived.set(cell, true);
    }

    /// Whether a callable is a class constructor, and so may only be reached
    /// through `new`.
    ///
    /// # Why this lives in `callables` and not in a table of its own
    ///
    /// Because **every call asked both questions and paid for two probes.**
    /// `functions::called` asks this before anything else, and
    /// `functions::invoke` asks `callable_at` a moment later — two `Aside<T>`,
    /// each a `Vec<Option<T>>` indexed by the same cell, so two bounds checks
    /// and two cache lines to answer one call. Merging them makes the second
    /// question free: the line is already in cache from the first.
    ///
    /// Measured 2026-08-28 by ablating the check entirely, which bounds what
    /// merging can recover: `c.m(a)` −10.0%, `Number.isInteger` −7.7%,
    /// `Object.is` −6.9%, `set.has(7)` −5.1%, with the static-call control at
    /// −0.3%. About 2 ns on every call in every program.
    ///
    /// Not the borrow — `action-table-2026-08-26.md` §4 implemented borrow
    /// merging in full and it bought nothing. It is the second *table*.
    pub(super) fn is_class_constructor(&self, cell: u32) -> bool {
        self.callables
            .copied(cell)
            .is_some_and(|(_, _, class_constructor)| class_constructor)
    }

    /// Records that it is.
    ///
    /// Keeps the code and environment already recorded: the class lowering
    /// emits `closure_new` before `mark_class_constructor`, so the entry
    /// exists by the time this runs. A cell with no entry is not callable and
    /// cannot be reached by a call, so there is nothing for the flag to
    /// protect — recording it against a zero code address would invent a
    /// callable instead.
    pub(super) fn mark_class_constructor(&mut self, cell: u32) {
        if let Some((code, environment, _)) = self.callables.copied(cell) {
            self.callables.set(cell, (code, environment, true));
        }
    }

    /// The primitive a wrapper object stands for, if it is one.
    pub(super) fn boxed_at(&self, cell: u32) -> Option<u64> {
        self.boxed.copied(cell)
    }

    /// Records that a cell wraps a primitive.
    pub(super) fn set_boxed(&mut self, cell: u32, primitive: u64) {
        self.boxed.set(cell, primitive);
    }

    /// Records that a cell calls this code with this environment.
    ///
    /// Preserves the class-constructor flag if one is already there. Nothing
    /// re-marks a callable today, but a `set` that silently cleared a
    /// refusal-to-call would turn `class C {}; C()` from a `TypeError` into a
    /// constructor running with no `this`, which is the kind of thing that
    /// should not depend on call order.
    pub(super) fn mark_callable(&mut self, cell: u32, code: u64, environment: u64) {
        let class_constructor = self
            .callables
            .copied(cell)
            .is_some_and(|(_, _, flag)| flag);
        self.callables.set(cell, (code, environment, class_constructor));
    }

    /// The text a reference names, if it names one.
    ///
    /// A reference is a REGION index now, uniformly — for a string as much as
    /// for an object. Its cell holds the string's type in the header and where
    /// the text is in its first slot; the text itself is in the slab, because a
    /// string is any length and a cell is 64 bytes.
    ///
    /// That indirection is not a compromise. String identity and string data are
    /// separate things in every engine that moves either one, and putting the
    /// identity in the region is what lets one reference space serve both kinds.
    pub fn text_at(&self, reference: u32) -> Option<&Str> {
        if self.region.type_of(reference)? as usize != self.text_type.index() {
            return None;
        }
        let slot = self.region.field(reference, 0)? as u32;
        self.cells.at(Slot(slot)).ok()
    }

    /// The key a string cell's text has, without copying that text.
    ///
    /// # Why this exists rather than `intern(text_at(cell))`
    ///
    /// Because that does not compile, and the shape it was written as instead
    /// cost an allocation on every computed property access. `text_at` borrows
    /// the whole context; `Interner::intern` needs the interner and the key
    /// registry mutably. So the caller cloned the `Str` to break the borrow —
    /// a full copy of the string's buffer, per access, discarded immediately.
    ///
    /// Written out as three **disjoint field** borrows, the borrow checker
    /// accepts it: `cells` is read while `interner` and `keys` are written, and
    /// they are different fields. That is the whole trick, and it is why this is
    /// a method on the context rather than a helper beside the caller — only a
    /// function that can name the fields can split them.
    ///
    /// `None` for a reference that is not a string, which is what makes the
    /// caller fall through to the general conversion instead of guessing.
    pub(super) fn key_of_text_cell(&mut self, reference: u32) -> Option<crate::object::Key> {
        if self.region.type_of(reference)? as usize != self.text_type.index() {
            return None;
        }
        let slot = self.region.field(reference, 0)? as u32;
        let text = self.cells.at(Slot(slot)).ok()?;
        // The memo the text carries, before the interner is asked. Without it
        // this hashes the string on EVERY access, which made a property read
        // cost three nanoseconds per character of the name — see `Str::key` for
        // the measurement and for why the memo lives on the text rather than on
        // the cell.
        //
        // `KeyRegistry::key` rather than a cast: it answers `None` for a number
        // the registry never issued, so a memo that could not possibly be right
        // falls through to the honest resolution instead of inventing a key.
        // Nothing can put a wrong number there today — `remember_key` is the
        // only writer and it writes what `intern` just answered — and the check
        // costs a comparison, which is the right price for keeping that true.
        if let Some(number) = text.remembered_key()
            && let Some(found) = self.keys.key(number)
        {
            return Some(crate::object::Key::Name(found));
        }
        let minted = self.interner.intern(text, &mut self.keys);
        // Through the shared reference the borrow above already holds: `intern`
        // wants `self.interner` and `self.keys` mutably while `text` is borrowed
        // out of `self.cells`, which Rust allows because they are different
        // fields — and asking for the slab mutably to write this back would not
        // be. `Str::key` is a `Cell` for exactly that reason.
        text.remember_key(minted.index() as u32);
        Some(crate::object::Key::Name(minted))
    }

    /// The string cell for a text that is a property KEY, built at most once.
    ///
    /// # Why this is not [`Context::intern_value`] with a cache bolted on
    ///
    /// Because interning a `Str` and interning a *key* are different questions.
    /// `intern_value` builds a cell for any text — most of them are values a
    /// program computed, distinct every time, and remembering those would be a
    /// leak wearing a cache's clothes. A key is the opposite: the interner
    /// already holds its text forever, the set is closed by the program's own
    /// property names, and enumeration hands the same one back over and over.
    ///
    /// The hash is what replaces the allocation. Resolving the text to its
    /// `Key` costs one hash of a short string; building the cell costs a slab
    /// insert, a region allocation and two field writes — and an allocation is
    /// what brings the next collection closer, which is the cost that does not
    /// show up in a per-call number.
    ///
    /// Sound because a JavaScript string is immutable and has no observable
    /// identity: `===` compares text, and there is no operation that can tell
    /// two equal strings apart. So one cell per distinct key is not a
    /// deduplication a program can notice.
    /// The one string cell a key's text lives in, named by the KEY itself.
    ///
    /// The second door onto the same table [`Context::key_text_value`] fills, and
    /// it exists because the first one is reached the long way round. A shape
    /// already holds its properties AS keys, so enumeration had the number in
    /// hand, converted it to text, and handed the text to a function whose first
    /// act is to hash it back into the number.
    ///
    /// Measured 2026-08-25 by ADDING one extra round trip per key rather than by
    /// removing the only one — an idempotent call, so the answer cannot change
    /// and what moves is the cost alone: `Object.keys` of an eight-property
    /// object went from 2 023 ns to 2 488, which is **58 ns per key** for the
    /// text and its hash.
    ///
    /// Two doors rather than one that takes an enum, for the reason
    /// `functions::call` and `call_counted` are two: a caller holding text has
    /// no key and a caller holding a key has no text, and neither should have to
    /// manufacture the other to ask.
    pub(super) fn key_value(&mut self, key: rts_cranelift::shape::Key) -> u64 {
        let named = crate::object::Key::Name(key);
        if let Some(held) = self.key_texts_as_values.get(&named) {
            return *held;
        }
        // A key the interner does not know has no text to put on the heap. It
        // cannot happen for a key that came out of a shape — the interner is
        // what minted it — and answering `undefined` is what the callers of this
        // already do for a property that turns out not to be there.
        let Some(text) = self.interner.text(key).cloned() else {
            return super::objects::undefined_of(self);
        };
        let value = self.intern_value(text).bits();
        self.key_texts_as_values.insert(named, value);
        value
    }

    /// Rebuilds the by-address index of [`Context::function_names`].
    ///
    /// # Why an index and not a second table
    ///
    /// `function_names` stays the authority — this is DERIVED from it, and it is
    /// rebuilt whenever that is replaced or extended so the two cannot drift.
    /// Everything in it is a number, deliberately: an interned key rather than
    /// the `String`, so nothing here has to be kept alive by a collection and
    /// nothing has to be cloned to be read.
    ///
    /// # What it is worth
    ///
    /// `closure_new` runs once per CLOSURE and not once per function, so an
    /// arrow written inside a loop reached it every iteration — and what it did
    /// there was walk `function_names` linearly, clone a `String` out of it, and
    /// convert that to UTF-16 to hash it. The walk alone was measured at 0.42 ns
    /// per entry on 2026-08-25: 733.7 ns with two functions in the table against
    /// 817.6 with two hundred, which is a cost that grows with the size of the
    /// PROGRAM rather than with what it does.
    ///
    /// The name is interned HERE, once per function, so the hot path holds a
    /// number and reads the text — when it needs it at all — out of the cache
    /// `key_value` already keeps and `roots` already scans.
    /// The layout a callable of this shape ends up at, once one has been built.
    ///
    /// # What it is for
    ///
    /// `closure_new` allocates at the EMPTY layout and then writes `prototype`,
    /// `name` and `length` through `objects::put`, which takes a shape
    /// transition per write. Every closure of every function takes the same
    /// three, so the layout it arrives at is the same handful of answers for the
    /// whole program — and `put` recomputed them per closure. Measured
    /// 2026-08-25 by ablation: the two `put`s for `name` and `length` were 78 ns
    /// of a 218 ns closure.
    ///
    /// # Why the FIRST closure records it rather than something computing it
    ///
    /// Because a template computed here would be a second statement of what a
    /// property write does — which slot a key lands in, which representation the
    /// shape records, what the header becomes — and the two would drift the day
    /// `put` changed. So the first closure of each shape goes through `put`
    /// exactly as before and the result is REMEMBERED: the template is a
    /// recording of the real path, not a reimplementation of it, and a closure
    /// built from one is byte-identical to one built the long way.
    ///
    /// Indexed by [`Self::callable_template_at`]'s two questions, because a
    /// callable that gets a `prototype` and one that does not are different
    /// layouts, and so are a described one and an anonymous one.
    pub(super) fn callable_template(&self, at: usize) -> Option<CallableTemplate> {
        self.callable_templates[at]
    }

    /// Remembers the layout a freshly built callable arrived at.
    ///
    /// Reads the slots back out of the SHAPE rather than being told them, so a
    /// caller cannot record a position `put` did not actually use.
    pub(super) fn record_callable_template(&mut self, at: usize, cell: u32, keys: &[Option<crate::object::Key>; 3]) {
        if self.callable_templates[at].is_some() {
            return;
        }
        let Some(ty) = self.region.type_of(cell) else {
            return;
        };
        let Some(shape) = self.shape_of(ty) else {
            return;
        };
        let mut slots = [None; 3];
        for (at, key) in keys.iter().enumerate() {
            let Some(crate::object::Key::Name(named)) = key else {
                continue;
            };
            let Some(slot) = self.shapes.slot_of(shape, *named) else {
                // A key `put` declined to store — a frozen cell, a refusal —
                // means this cell is not a template for anything. Recording a
                // partial one would hand later closures a layout missing a
                // property they are supposed to have.
                return;
            };
            slots[at] = Some(slot);
        }
        self.callable_templates[at] = Some(CallableTemplate { ty, slots });
    }

    /// What the emitter recorded about the function compiled at `code`.
    ///
    /// The interned name key, the arity, and whether `new` may reach it. `None`
    /// for an address nothing described — `rts-napi` and `eval` mint callables
    /// the emitter never saw, and every caller here treats that as "assume the
    /// permissive answer" rather than as an error.
    pub(super) fn described_at(
        &self,
        code: u64,
    ) -> Option<(rts_cranelift::shape::Key, u32, bool, bool)> {
        self.function_by_code.get(&code).copied()
    }

    pub(super) fn index_functions_by_code(&mut self) {
        let described: Vec<(u64, Str, u32, bool, bool)> = self
            .function_names
            .iter()
            .map(|(at, name, arity, has_prototype, constructs)| {
                (*at, Str::from_str(name), *arity, *has_prototype, *constructs)
            })
            .collect();
        self.function_by_code.clear();
        for (at, name, arity, has_prototype, constructs) in described {
            let key = self.interner.intern(&name, &mut self.keys);
            self.function_by_code
                .insert(at, (key, arity, has_prototype, constructs));
        }
    }

    pub(super) fn key_text_value(&mut self, text: &Str) -> u64 {
        let key = crate::object::Key::Name(self.interner.intern(text, &mut self.keys));
        if let Some(held) = self.key_texts_as_values.get(&key) {
            return *held;
        }
        let value = self.intern_value(text.clone()).bits();
        self.key_texts_as_values.insert(key, value);
        value
    }

    /// Put a string on the heap and return the value naming it.
    pub fn intern_value(&mut self, text: Str) -> Value {
        let length = text.len();
        let size = crate::heap::STRIDE;
        let ty = self.text_type.index() as u32;
        // Reserve the region cell before putting the text in the slab. If the
        // region is full, collection may run here; inserting the slab entry
        // first would leave a text with no region root while that collection
        // scans the heap.
        let cell = super::alloc::alloc_or_die(self, size, ty);
        let slot = self.cells.insert(text).slot();
        self.region
            .set_field(cell, 0, u64::from(slot.0))
            .expect("a string cell has a first slot");
        // The LENGTH as a VALUE, not as an integer: a cached read hands back what
        // the slot holds, so what the slot holds must be a JavaScript value. It
        // was a raw `u64` for one build and every length read back as a denormal —
        // 2 answered 1e-323 — which is what a bit pattern looks like when it is
        // read as the double it is not.
        //
        // Beside the slab position, so that reading it is a load rather than
        // a slab lookup and — the point — so an inline cache can answer for it.
        // A string is immutable, so this is written once and can never drift
        // from what it describes; that is what makes storing it safe here and
        // would not make it safe for an array.
        self.region
            .set_field(cell, crate::entry::TEXT_LENGTH_SLOT, Value::from_f64(length as f64).bits())
            .expect("a string cell has a second slot");
        Value::from_slot(cell)
    }

    /// The primitive string for one UTF-16 code unit.
    ///
    /// The common Latin-1 range is immutable and finite, so retaining one value
    /// per unit removes the repeated slab and region allocation from string
    /// indexing and `charAt`. A unit above 255 still uses the ordinary path so
    /// lone surrogates and all other UTF-16 values remain representable.
    pub(super) fn single_unit_text(&mut self, unit: u16) -> u64 {
        let index = usize::from(unit);
        if index < self.single_unit_texts.len() {
            if let Some(value) = self.single_unit_texts[index] {
                return value;
            }
            let value = self
                .intern_value(Str::owning_latin1(vec![unit as u8]))
                .bits();
            self.single_unit_texts[index] = Some(value);
            return value;
        }
        self.intern_value(Str::from_utf16(&[unit])).bits()
    }

    /// Whether two slots hold equal text.
    ///
    /// What `===` needs and cannot answer alone: two strings are equal when
    /// their text is, however they were allocated, while two objects are equal
    /// only when they are the same object.
    pub fn same_text(&self, left: u32, right: u32) -> bool {
        match (self.text_at(left), self.text_at(right)) {
            (Some(a), Some(b)) => a.same_units(b),
            _ => false,
        }
    }
}

impl Context {
    /// The key a name the runtime itself knows has.
    ///
    /// `length` and `prototype` are properties this crate reads by name rather
    /// than by a number a compilation resolved, because it is the runtime that
    /// wants them — an array answers `length` whether or not the program ever
    /// wrote it, and `new` reads `prototype` on a function the program may
    /// never have touched.
    ///
    /// Interned rather than held as a constant: the number is whatever the
    /// registry issued, and that registry was seeded from what the compilation
    /// resolved. A program that mentions the name already put it there and this
    /// finds the same number; one that never does mints one nothing else uses,
    /// which costs a key and changes no answer.
    ///
    /// One function because it was two, and "intern a name the runtime knows"
    /// is one rule — the second copy is the one that would have interned
    /// against a different registry the day there were two.
    pub(super) fn well_known(&mut self, name: &str) -> crate::object::Key {
        // Remembered for the names asked on paths that run per OPERATION rather
        // than per program. `length` is the sharpest: `reconcile_length` asks
        // for it before every property write in the program, to find out
        // whether this is the write that truncates an array — so a construction
        // with four fields built the text as UTF-16 and hashed it four times,
        // for an answer that cannot change within a run. `prototype` is read by
        // every `new`, and the other three are stamped onto every typed array.
        //
        // A static match over the nine names, not a `HashMap<String, Key>`,
        // which would hash the name to avoid hashing the name. Anything not on
        // the list falls through to the intern exactly as before, and putting a
        // name on it is only ever a speed decision — the answer is identical
        // either way, because the intern is idempotent.
        let held = match name {
            "length" => Some(0),
            "prototype" => Some(1),
            "byteLength" => Some(2),
            "byteOffset" => Some(3),
            "buffer" => Some(4),
            "toJSON" => Some(5),
            "name" => Some(6),
            "constructor" => Some(7),
            super::symbol::SPECIES => Some(8),
            "value" => Some(9),
            "done" => Some(10),
            super::symbol::ITERATOR => Some(11),
            "next" => Some(12),
            "return" => Some(13),
            super::symbol::HAS_INSTANCE => Some(14),
            "index" => Some(15),
            "input" => Some(16),
            "groups" => Some(17),
            "lastIndex" => Some(18),
            "indices" => Some(19),
            _ => None,
        };
        if let Some(at) = held
            && let Some(key) = self.well_known_keys[at]
        {
            return key;
        }
        let text = Str::from_str(name);
        let key = crate::object::Key::Name(self.interner.intern(&text, &mut self.keys));
        if let Some(at) = held {
            self.well_known_keys[at] = Some(key);
        }
        key
    }

    /// A string the runtime itself needs as a VALUE, built at most once.
    ///
    /// The counterpart of [`Self::well_known`] for the other half of the same
    /// problem: that one saves hashing text, this one saves an ALLOCATION.
    /// `intern_value` does not intern despite its name — it inserts into the
    /// slab and calls the allocator — so a name built per operation is a cell
    /// per operation, and cells are what a collection has to walk.
    ///
    /// Falls through for a name not on [`super::CACHED_TEXTS`], so a caller
    /// that hands it something else gets the ordinary behaviour.
    pub(super) fn well_known_text(&mut self, name: &str) -> u64 {
        let held = match name {
            "toJSON" => Some(0),
            "" => Some(1),
            super::symbol::HAS_INSTANCE => Some(2),
            _ => None,
        };
        if let Some(at) = held
            && let Some(value) = self.well_known_texts[at]
        {
            return value;
        }
        let value = self.intern_value(Str::from_str(name)).bits();
        if let Some(at) = held {
            self.well_known_texts[at] = Some(value);
        }
        value
    }
}
