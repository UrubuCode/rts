//! The two composites: `[…]` and `{…}`, and the runs of primitives inside them.

use super::plan::{Lent, Member, plain_properties, member_value};
use super::shape::{Shape, is_primitive, shape_of};
use super::to_json::HookKey;
use super::Writer;
use super::super::hooks::Replacer;
use super::super::super::{Context, with_current};
use crate::value::Value;

impl Writer {
    /// `[…]`.
    ///
    /// # Why each element is a fresh `[[Get]]` and not a clone of the store
    ///
    /// `SerializeJSONArray` reads `len` ONCE and then performs an ordinary
    /// property read per index — `array[i]`, the same operation `array[1]`
    /// compiles to — so an index that is an ACCESSOR runs its getter, and a
    /// getter or a `toJSON` that shrinks the array is observable on every
    /// index still to come: `arr = [0,1,2,3]` with a getter at index 1 that
    /// sets `arr.length = 2` serialises as `[0,"one",null,null]` — `len` was
    /// still 4, but reading indices 2 and 3 after the shrink answers
    /// `undefined`, which serialises as `null` here exactly as a hole does.
    ///
    /// A clone of the element store, taken once, cannot show any of that: it
    /// answers what the array held before the walk started, so a shrink mid
    /// walk left the old values printed — a well-formed but wrong array.
    pub(super) fn array(&mut self, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // A root for the whole walk: every read below is a call back into the
        // runtime, and the array itself is otherwise named by nothing a
        // conservative stack scan can see once `length` has been read out of
        // it.
        let anchor = super::super::super::external::hold_current(Value::from_slot(cell).bits());
        let object = Value::from_slot(cell).bits();
        // `LengthOfArrayLike`, read ONCE — see this function's own
        // documentation for why every element after it is still a live read.
        let length = with_current(|context| {
            let key = super::super::super::computed::length_key(context);
            super::super::super::objects::own_property(context, cell, key)
                .and_then(|value| value.numeric())
                .map_or(0usize, |number| number.max(0.0) as usize)
        });
        self.ascii("[");
        let mut at = 0;
        while at < length {
            if super::super::super::throw::in_flight() {
                break;
            }
            // A RUN of primitive elements in one borrow — see
            // [`Self::run_of_elements`]. Still a live read of the store at the
            // moment each index is reached: a run ends at the first element
            // that could run anything, so a shrink by an element's `toJSON` is
            // seen by the run after it exactly as the ordinary read sees it.
            if self.unobserved() {
                at = with_current(|context| self.run_of_elements(context, cell, at, length, depth));
                if at >= length {
                    break;
                }
            }
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);
            // The ordinary indexed read: a hole and an `undefined` element
            // both answer `undefined` here exactly as [`super::super::super::array::visible`]
            // says a compiled `a[k]` does, which is what keeps this agreeing
            // with the language about what an element IS at the moment it is
            // actually read, rather than at the moment the walk began.
            let element = super::super::super::computed::get_indexed(
                object,
                Value::from_f64(at as f64).bits(),
            );
            let held = self.hooked(object, element, HookKey::Index(at));
            if !self.write(held, depth + 1) {
                self.ascii("null");
            }
            at += 1;
        }
        if length > 0 {
            self.newline(depth);
        }
        // The array is read for the last time above.
        super::super::super::external::release_current(anchor);
        self.ascii("]");
        self.leave();
    }

    /// `{…}`.
    pub(super) fn object(&mut self, value: u64, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // A plain object serialised straight off its shape, with no key list on
        // the heap and no read by text. `plain_properties` says which objects
        // those are and why the four it refuses are refusals of substance.
        //
        // Not attempted at all when a list replacer is in force: that names the
        // members and their order itself, so the object's own enumeration is not
        // consulted — a fast path over the shape would answer the wrong members
        // rather than the same ones faster.
        if !matches!(self.replacer, Replacer::List(_)) {
            if self.plans.len() <= depth {
                self.plans.resize_with(depth + 1, || None);
            }
            // TAKEN OUT for the walk and put back after it: `plain` descends
            // into `self`, so the members cannot stay borrowed from it.
            let mut plan = self.plans[depth].take();
            let lent = with_current(|context| plain_properties(context, cell, &mut plan));
            let walked = match (&lent, &plan) {
                (Some(Lent::Own(keys)), _) => {
                    self.plain(value, keys, depth);
                    true
                }
                (Some(Lent::Planned), Some((_, Some(keys), _))) => {
                    self.plain(value, keys, depth);
                    true
                }
                _ => false,
            };
            self.plans[depth] = plan;
            if walked {
                self.leave();
                return;
            }
        }
        // The runtime's own enumeration, which is what `Object.keys` and
        // `for-in` walk. A second walk of the layout here would be a second
        // answer to "what order", and the two would drift the first time one
        // was fixed.
        // HELD, and this is a correctness fix rather than a nicety.
        //
        // `own_keys` answers an ARRAY on the heap, and the loop below clones its
        // elements into a Rust `Vec` and then allocates — a string per key, a
        // value per member — while walking that clone. The array itself is dead
        // to Rust after the clone, so nothing keeps it in a register and the
        // conservative stack scan cannot see it; the cloned references live in a
        // `Vec`'s buffer, which is on the Rust heap and is not scanned at all.
        //
        // A collection triggered by one of those allocations therefore freed the
        // key strings this loop was about to read, and the cells came back out
        // of the free list as something else. Measured before this: 31 wrong
        // results per 300 000 `JSON.stringify` calls on a four-member object —
        // a key duplicated or dropped, silently, in valid-looking JSON.
        //
        // `external` is a root (`roots.rs`), so holding the array keeps it and
        // everything it reaches alive for exactly as long as this needs them.
        // Released at the end of the function rather than at the end of the
        // loop, because the last key is read after the last iteration.
        // A list replacer names the members and their order; the object's own
        // enumeration is not consulted at all, which is what makes
        // `stringify(o, ["c", "a"])` answer `{"c":…,"a":…}` for an object whose
        // own order is the other way round. Built as a heap array so the hold
        // below covers both cases with one rule rather than two.
        let names = match &self.replacer {
            Replacer::List(keys) => with_current(|context| {
                let interned: Vec<u64> = keys
                    .iter()
                    .map(|key| super::super::hooks::interned(context, key))
                    .collect();
                super::super::super::array::built_in(context, interned)
            }),
            _ => super::super::super::array::own_keys(value),
        };
        let anchor = super::super::super::external::hold_current(names);
        let names = with_current(|context| {
            Value(names)
                .as_slot()
                .and_then(|cell| context.elements_at(cell).cloned())
                .unwrap_or_default()
        });

        self.ascii("{");
        let mut written = false;
        for name in names {
            if super::super::super::throw::in_flight() {
                break;
            }
            // Through the ordinary read, so a member that is an accessor runs
            // its getter — which is what `stringify` observably does, and what
            // reading the slot directly would have skipped.
            // Through the ordinary read, so a member that is an accessor runs
            // its getter — which is what `stringify` observably does, and what
            // reading the slot directly would have skipped.
            //
            // Reading it by KEY instead, in one borrow, was written and
            // MEASURED and reverted: `get_indexed` already takes the fast route
            // for a name that is a string cell, so collapsing the borrows moved
            // `{a:1}` from 1942 ns to 2084 ns — inside the run-to-run spread on
            // this machine, which is to say it bought nothing and cost a second
            // path through this loop. Whatever the ~800 ns per member is, it is
            // not this.
            //
            // # FOUND, 2026-08-23, and it is not a JSON problem
            //
            // The per-member cost is ~480 ns, not 800 — the earlier figure came
            // from dividing a fixed cost by a member count. Measured by varying
            // the shape instead of the count:
            //
            //   JSON.stringify(42)          225 ns    the floor for any call
            //   JSON.stringify({})          695 ns    +470 just for being an object
            //   JSON.stringify({a:1})      1417 ns
            //   JSON.stringify({a..h})     4763 ns    ~480 per member
            //   JSON.stringify([1,2,3,4])   778 ns    ~74 per ELEMENT
            //
            // An array element and an object member write the same number, and
            // the member costs six to ten times the element. The whole
            // difference is the KEY, and the key's cost is not here either:
            //
            //   o.a  + o.b  + o.c  + o.d     (literal keys)     39 ns
            //   o[k] x4, k from Object.keys (string keys)     1086 ns
            //
            // Twenty-seven times, and it SCALES WITH THE LENGTH OF THE NAME —
            // 115 ns for a one-character key, 331 for 64 characters, 891 for
            // 256. That is `Context::key_of_text_cell`, which ends in
            // `interner.intern(text, …)`: a HASH OF THE TEXT on every access.
            //
            // So this loop is not slow; reading a property by a string is, and
            // this loop does it once per member. The fix belongs there and is
            // researched rather than guessed — V8 caches the hash in the
            // string's own header and internalizes key strings so lookup
            // compares pointers; SpiderMonkey canonicalizes to atoms and added a
            // cache of recently-atomized strings for exactly this.
            //
            // LANDED as `Str::key`, and the escalation with name length is gone:
            // a 256-character key went from 798 ns to 63, a one-character key
            // from 104 to 63, and a literal read stayed at 26.
            //
            // THIS LOOP moved much less — 4 160 ns to 3 891 — and the reason is
            // worth writing down here rather than being rediscovered: `own_keys`
            // hands back FRESH string cells, so the memo is cold on every call.
            // The key resolution is no longer the cost; building the key strings
            // is. That is the next question for this file, and it is a different
            // one.
            let held = super::super::super::computed::get_indexed(value, name);
            let key = with_current(|context| super::super::super::text::to_text(context, Value(name)));
            let Some(key) = key else {
                continue;
            };
            // The hooks run HERE, before the key is written, because they are
            // what decides whether there is a value at all: a `toJSON` or a
            // replacer answering `undefined` skips the member, and asking after
            // the key was emitted produced `{"drop":}`.
            //
            // Classified once here and again inside `write` — one extra borrow
            // per member, and it buys the separator staying correct: a member
            // skipped after its comma was emitted is a trailing comma, which is
            // not JSON either.
            //
            // `name` is already the string key, straight from `own_keys`, so
            // this is the same value `toJSON` must see with no second
            // conversion to disagree with the first.
            let held = self.hooked(value, held, HookKey::Given(name));
            // Classified once, the answer carried — see `plain`.
            if super::super::super::throw::in_flight() {
                return;
            }
            let shape = with_current(|context| shape_of(context, held));
            if matches!(shape, Shape::Absent) {
                continue;
            }
            if written {
                self.ascii(",");
            }
            written = true;
            self.newline(depth + 1);
            self.quoted(&key);
            self.ascii(":");
            if !self.indent.is_empty() {
                self.ascii(" ");
            }
            self.write_shape(shape, held, depth + 1);
        }
        if written {
            self.newline(depth);
        }
        // The keys are read for the last time above, so the hold ends here.
        super::super::super::external::release_current(anchor);
        self.ascii("}");
        self.leave();
    }

    /// The members of a plain object, read one at a time off its layout.
    ///
    /// Mirrors the general loop in [`Self::object`] step for step — the throw
    /// check, the hooks before the key is written, the `Absent` skip that keeps
    /// a trailing comma from happening, the separator — and differs only in
    /// where the key and the value come from. Written as its own function so
    /// that the difference is the only thing a reader has to compare, rather
    /// than a second copy of the whole rule to keep in agreement with the first.
    ///
    /// `keys` holds numbers, never references, so nothing here is invisible to
    /// the collector while an allocation happens. That is why the general path's
    /// `external::hold_current` has no counterpart in this one: there is no
    /// heap array to keep alive, because none was made.
    fn plain(&mut self, value: u64, keys: &[Member], depth: usize) {
        self.ascii("{");
        let mut written = false;
        let Some(cell) = Value(value).as_slot() else {
            return self.ascii("}");
        };
        let shape = with_current(|context| context.shape_of(context.region.type_of(cell)?));
        let mut at = 0;
        while at < keys.len() {
            if super::super::super::throw::in_flight() {
                break;
            }
            // A RUN of primitive members in one borrow — see
            // [`Self::run_of_members`]. It stops AT the first member that is
            // not one, which is the member the rest of this pass is about.
            if self.unobserved() {
                at = with_current(|context| {
                    self.run_of_members(context, cell, shape, keys, at, depth, &mut written)
                });
                if at >= keys.len() {
                    break;
                }
            }
            let member = &keys[at];
            at += 1;
            // The member is an own data property of an object
            // `plain_properties` proved has no accessors and no proxy, so
            // reading it runs nothing and can allocate nothing.
            let Some(held) = with_current(|context| member_value(context, cell, shape, member)) else {
                continue;
            };
            let held = self.hooked(value, held, HookKey::Named(member.key));
            // CLASSIFIED ONCE, and the answer carried to the write.
            //
            // The test exists to satisfy rule 8 — a `toJSON` or a replacer
            // answering `undefined` must not produce `{"drop":}` — and it was
            // asking `shape_of` solely to see `Absent`, after which `write`
            // asked the identical question again. Passing the decision along
            // removes the second borrow and the second classification without
            // removing the question.
            if super::super::super::throw::in_flight() {
                return;
            }
            let classified = with_current(|context| shape_of(context, held));
            if matches!(classified, Shape::Absent) {
                continue;
            }
            if written {
                self.ascii(",");
            }
            written = true;
            self.newline(depth + 1);
            with_current(|context| self.label(context, member));
            self.ascii(if self.indent.is_empty() { ":" } else { ": " });
            self.write_shape(classified, held, depth + 1);
        }
        if written {
            self.newline(depth);
        }
        self.ascii("}");
    }

    /// Writes members from `at` for as long as they are primitives, and answers
    /// the index it stopped at.
    ///
    /// # Why a run and not a member
    ///
    /// Writing a primitive runs no user code and allocates no cell, so nothing
    /// can change between one member and the next: the borrow that read the
    /// first is as good for the second. It was a borrow per member — a
    /// thread-local read and a `RefCell` flag each — and before that five.
    /// Measured 2026-09-19, `target/release/rts.exe`: eight numeric members
    /// cost 110 ns each where an array element cost 45.
    #[allow(clippy::too_many_arguments)]
    fn run_of_members(
        &mut self,
        context: &mut Context,
        cell: u32,
        shape: Option<rts_cranelift::shape::ShapeId>,
        keys: &[Member],
        mut at: usize,
        depth: usize,
        written: &mut bool,
    ) -> usize {
        let indented = !self.indent.is_empty();
        while let Some(member) = keys.get(at) {
            let Some(found) = member_value(context, cell, shape, member) else {
                // Gone since the plan was made: skipped, as the ordinary read
                // skips it.
                at += 1;
                continue;
            };
            if !is_primitive(context, found) {
                break;
            }
            if *written {
                self.ascii(",");
            }
            *written = true;
            self.newline(depth + 1);
            self.label(context, member);
            self.ascii(if indented { ": " } else { ":" });
            self.direct(context, found);
            at += 1;
        }
        at
    }

    /// Writes elements from `at` for as long as they are primitives, and
    /// answers the index it stopped at. [`Self::run_of_members`] has the
    /// argument; what is particular to an array is what ends a run.
    ///
    /// A HOLE ends it, because a hole reads through the prototype chain and
    /// that is the ordinary read's to answer. So does an accessor anywhere on
    /// the cell, asked once per run: nothing in a run can define one.
    fn run_of_elements(&mut self, context: &Context, cell: u32, mut at: usize, length: usize, depth: usize) -> usize {
        if !context.ranked_accessors(cell).is_empty() {
            return at;
        }
        let Some(held) = context.elements_at(cell) else {
            return at;
        };
        while at < length {
            let Some(found) = held.get(at).copied() else {
                break;
            };
            if super::super::super::array::is_hole(context, found) || !is_primitive(context, found) {
                break;
            }
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);
            self.direct(context, found);
            at += 1;
        }
        at
    }
}
