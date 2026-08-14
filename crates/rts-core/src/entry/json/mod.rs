//! `JSON`: text out of a value, and a value out of text.
//!
//! # Why a folder and not one file
//!
//! Reading text and writing it are two grammars, not two halves of one. The
//! writer walks the heap and never looks at a character it did not put there;
//! the reader walks characters and never touches the heap until it has a whole
//! tree. Sharing a file would have put the 500-line ceiling in charge of which
//! of them got documented.
//!
//! # The rule the whole module is shaped around
//!
//! `with_current` holds a `RefCell` borrow for the length of its body, and the
//! entry points this needs — `own_keys`, `get_indexed`, `array_new` — each take
//! one of their own. Nesting them panics, and a panic out of an `extern "C"`
//! frame aborts the process rather than failing a test.
//!
//! So neither direction holds a borrow across a call. The writer classifies a
//! value in one short borrow, gives it back, and only then reads the next one;
//! the reader parses into [`read::Node`], a tree of plain Rust with no heap in
//! it at all, and materialises afterwards. The second is the more interesting
//! choice: a parser that allocated as it went would be correct and would have
//! one borrow per allocation interleaved with recursion, which is exactly the
//! shape in which this mistake hides. A pure tree makes the discipline
//! trivially checkable — there is one function that allocates, and it is at the
//! bottom of this file.
//!
//! # All four hooks run
//!
//! This section used to list three of them as absent — `toJSON`, `replacer`,
//! `reviver` — under one argument: each is a call into user code per member,
//! and the walk that calls them is the walk this module keeps flat. The
//! argument was about *cost*, and it was answered by paying it once:
//! `to_json_of` probes, releases and calls, and both other hooks reach user
//! code through the same shape. [`hooks`] holds the two that arrived last.
//!
//! What the walk owes them is rule 8 of `crates/rts-core/README.md`: after a
//! call, ask whether it threw before believing the answer. The reviver is where
//! that matters most — its answer decides whether a member is kept or DELETED,
//! so a reviver that raises would otherwise erase the tree it was reading.
//!
//! **`space`** was implemented first and alone, because it is the one that runs
//! nothing: pure formatting, never changing which members are written.
//!
//! # Where this still answers instead of throwing
//!
//! A cycle is a `TypeError` in the specification and bad JSON is a
//! `SyntaxError`. Only the second is thrown here now — `parse` calls no user
//! code, so rule 8's discipline (`crates/rts-core/README.md`) is satisfied
//! trivially and `throw::syntax_error` is reachable from an entry point that
//! never held a borrow across it. The cycle case is different: `write` calls
//! BACK into user code (getters, `toJSON`) while `self.open` is live, and a
//! raise there would need every one of those call sites to check for a throw
//! before continuing the walk, which they do not yet. So a cycle still writes
//! `null` at the point it closes, and that is the narrower, still-true gap —
//! not "a throw needs a protected region", which stopped being true the day a
//! throw learned to leave one frame.

mod hooks;
mod read;
mod write;

use read::Node;

use super::objects::undefined_of;
use super::with_current;
use crate::text::Str;
use crate::value::Value;

/// How deep either direction descends before it stops.
///
/// Both grammars are recursive and recursion here is Rust's stack, which an
/// `extern "C"` frame cannot survive running out of. The reader answers a parse
/// error past this, which is a defined outcome; the writer answers `null`,
/// which is a stated divergence.
///
/// It was 200, and 200 was chosen as "deeper than any hand-written document".
/// That was the wrong shape of guess: a document exactly 200 deep is what a
/// program that generates one writes, and the limit refused it — measured, on
/// `json/claude-parse-deep-nesting.ts`, whose deepest case is 200 and which
/// answered a `SyntaxError` where every other engine answers a value. 512 is
/// still a ceiling and still ours; what changed is that it is no longer the
/// same order of magnitude as the documents programs actually build. Measured
/// at 512 in a DEBUG build, where the frames are largest: parse, revive and
/// stringify all return rather than overflowing.
pub(super) const DEPTH: usize = 512;

/// `JSON`.
#[rtse::class("JSON", namespace)]
impl Json {
    /// `JSON.stringify(value, replacer, space)`.
    ///
    /// Answers `undefined` — the value, not the text — when the argument has no
    /// JSON form at all, which is `undefined` itself and any function. That is
    /// the language: `JSON.stringify(undefined)` is not `"undefined"`, and the
    /// difference is what lets a caller test the answer rather than parse it.
    ///
    /// `replacer` is classified once, before the walk — see [`hooks::Replacer`].
    fn stringify(value: u64, replacer: u64, space: u64) -> u64 {
        let replacer = hooks::replacer_of(replacer);
        // The root's key is the empty string — the specification calls
        // `SerializeJSONProperty` with a synthetic holder `{"": value}`, which is
        // what makes `{ toJSON(key) { return key } }` answer `""` when it is the
        // whole argument to `stringify` rather than a member of something.
        //
        // The holder is only BUILT for a function replacer, which is the one
        // thing that can observe it: `toJSON` is called with the value as its
        // receiver, never with the holder.
        let holder = match replacer {
            hooks::Replacer::Function(_) => hooks::root_holder(value),
            _ => with_current(|context| undefined_of(context)),
        };
        let mut writer = write::Writer::new(write::indent_of(space), replacer);
        let root_key = with_current(|context| context.well_known_text(""));
        let value = writer.hooked(holder, value, super::json::write::HookKey::Given(root_key));
        match writer.write(value, 0) && !super::throw::in_flight() {
            true => {
                let units = writer.finish();
                with_current(|context| context.intern_value(Str::from_utf16(&units)).bits())
            }
            false => with_current(|context| undefined_of(context)),
        }
    }

    /// `JSON.parse(text, reviver)`.
    ///
    /// The reviver runs over the tree AFTER it is on the heap, never over
    /// [`read::Node`]: a reviver may answer any value at all, including objects
    /// the parsed tree has no way to describe, so a walk of the node tree would
    /// have to grow a second representation of everything the heap already has.
    fn parse(text: u64, reviver: u64) -> u64 {
        // `ToString` of the argument first, which is what the specification
        // says — `JSON.parse(5)` parses `"5"` and answers 5, and refusing a
        // non-string would refuse a call the language defines.
        let units = with_current(|context| {
            super::text::to_text(context, Value(text)).map(|text| text.units().collect::<Vec<u16>>())
        });
        let Some(units) = units else {
            return with_current(|context| undefined_of(context));
        };
        match read::parse_units(&units) {
            Some(node) => {
                let value = materialise(&node);
                match with_current(|context| super::modules::is_callable_in(context, reviver)) {
                    false => value,
                    // The same synthetic holder the writer's root uses, for the
                    // same reason: the reviver is called for the root too, and
                    // it needs a receiver and a key like every other member.
                    true => {
                        let holder = hooks::root_holder(value);
                        let root_key = with_current(|context| context.well_known_text(""));
                        hooks::internalized(holder, root_key, reviver)
                    }
                }
            }
            None => {
                // A `SyntaxError` a `catch` can see. This used to answer
                // `undefined` — the module header's stated gap from before a
                // native could raise at all — but that ground moved once rule 8's
                // discipline landed (see `throw.rs`): `parse` calls no user code,
                // so there is nothing to check first, and the reason to hold back
                // (a throw needing a protected region) no longer applies.
                super::throw::syntax_error("Unexpected token in JSON");
                with_current(|context| undefined_of(context))
            }
        }
    }
}

/// The heap value a parsed node names.
///
/// Every allocation in this module is reached from here, and every one of them
/// happens with no borrow held above it: a composite builds its children first,
/// each through its own borrows, and only then takes the one that stores them.
/// That ordering is the whole reason the parser answers a tree.
fn materialise(node: &read::Node) -> u64 {
    match node {
        Node::Null => with_current(|context| Value::from_singleton(context.singletons.null).bits()),
        Node::Bool(flag) => Value::from_bool(*flag).bits(),
        Node::Number(number) => Value::from_f64(*number).bits(),
        Node::Text(units) => {
            with_current(|context| context.intern_value(Str::from_utf16(units)).bits())
        }
        Node::Array(items) => {
            // ROOTED, and a loop rather than a `collect`: `materialise` is
            // recursive and every branch of it ALLOCATES, so the children built
            // so far are exposed between the steps of the loop that makes them
            // — named only by a `Vec` on the Rust heap, which no scan of ours
            // reaches. `array_new` below is a second exposure of the same list.
            // See `super::rooted`.
            let mut built = super::rooted::Rooted::new();
            for item in items {
                let value = materialise(item);
                built.values().push(value);
            }
            let array = super::array::array_new(built.len() as i64);
            let built = built.take();
            with_current(|context| {
                if let Some(cell) = Value(array).as_slot()
                    && let Some(elements) = context.elements_at_mut(cell)
                {
                    *elements = built;
                }
                array
            })
        }
        Node::Object(members) => {
            // The same, with the keys kept beside the guard: `Rooted` holds
            // values, and a `Str` is not one — it is text this function has not
            // interned yet, on the Rust heap where nothing can collect it.
            let mut values = super::rooted::Rooted::new();
            let mut keys: Vec<Str> = Vec::with_capacity(members.len());
            for (key, value) in members {
                let made = materialise(value);
                values.values().push(made);
                keys.push(Str::from_utf16(key));
            }
            // The guard stays ALIVE past this line, and `built` carries a copy
            // of the same words rather than taking them: everything below —
            // `native::plain`, and `objects::put` on the fallback path —
            // allocates, so the values have to remain registered until they are
            // written into the cell. Eight bytes a member to keep the existing
            // shape-building code untouched.
            let built: Vec<(Str, u64)> = keys
                .into_iter()
                .zip(values.as_slice().iter().copied())
                .collect();
            let made = with_current(|context| {
                let Some(cell) = super::native::plain(context) else {
                    return undefined_of(context);
                };
                // The layout is reached ONCE. A `put` per member is a shape
                // transition, a slot lookup, a type mint and a header write —
                // and every one of those types but the last is thrown away by
                // the next member. A parsed object knows all of its keys before
                // it stores any of them, which is exactly the case that does not
                // need to discover the layout one property at a time.
                //
                // Interned as a NAME, never as an index, which is what
                // `computed::property_key` does for every computed key — so
                // `JSON.parse("{\"0\":1}")[0]` finds what was stored. Routing
                // `"0"` through `Key::from_str` would file it among the
                // elements of an object that has none.
                let mut shape = context.shapes.root();
                let mut placed: Vec<(u32, u64)> = Vec::with_capacity(built.len());
                let mut fallback = false;
                for (key, value) in &built {
                    let named = context.interner.intern(key, &mut context.keys);
                    let Ok(grown) = context.shapes.transition(
                        shape,
                        named,
                        rts_cranelift::repr::Repr::Tagged,
                    ) else {
                        fallback = true;
                        break;
                    };
                    let Some(at) = context.shapes.slot_of(grown, named) else {
                        fallback = true;
                        break;
                    };
                    if at >= context
                        .region
                        .width_of(cell)
                        .unwrap_or(crate::heap::INLINE_SLOTS)
                    {
                        // Past the inline slots the value goes to the spill
                        // beside the cell, which `set_slot_value` does not
                        // reach. The general path does.
                        fallback = true;
                        break;
                    }
                    placed.push((at, *value));
                    shape = grown;
                }

                match fallback {
                    // A duplicate key, a refused transition, or more properties
                    // than fit inline. Rare, and the general path is right for
                    // all three rather than nearly right for two of them.
                    true => {
                        for (key, value) in built {
                            let key = crate::object::Key::Name(
                                context.interner.intern(&key, &mut context.keys),
                            );
                            super::objects::put(context, cell, key, value);
                        }
                    }
                    false => {
                        let link = context.prototype_at(cell);
                        let ty = context.typed_as(shape, link).index() as u32;
                        context.region.set_type(cell, ty);
                        for (at, value) in placed {
                            super::objects::set_slot_value(context, cell, at, value);
                        }
                    }
                }
                Value::from_slot(cell).bits()
            });
            // Released only now: the object holds every value, so the list has
            // nothing left to keep alive.
            drop(values);
            made
        }
    }
}
