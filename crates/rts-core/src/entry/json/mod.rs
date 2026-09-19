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
mod out;
pub(in crate::entry) mod read;
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
#[rtse::class("JSON", namespace, tag)]
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
        stringify(value, replacer, space)
    }

    /// `JSON.parse(text, reviver)`. See [`parse`].
    fn parse(text: u64, reviver: u64) -> u64 {
        parse(text, reviver)
    }
}

/// `JSON.stringify(value)` reached WITHOUT the name.
///
/// What the emitter calls once the whole program proves `JSON` is still the
/// primordial and no scope binds it — `math_random`'s argument, for a callee
/// whose path cost more than the draw did. Measured 2026-09-19 on
/// `target/release/rts.exe`: `JSON.stringify(42)` cost 238 ns where
/// `String(42)` cost 139, and the difference is a global read, a property read
/// through the chain cache and the generic call machinery.
///
/// One argument only. A replacer or an indentation is the ordinary call, which
/// is right for both and is not where a program's time goes.
#[rtse::entry]
pub fn json_stringify(value: u64) -> u64 {
    let absent = with_current(|context| undefined_of(context));
    stringify(value, absent, absent)
}

/// `JSON.parse(text)` reached without the name. See [`json_stringify`].
#[rtse::entry]
pub fn json_parse(text: u64) -> u64 {
    let absent = with_current(|context| undefined_of(context));
    parse(text, absent)
}

/// The body of `JSON.stringify`, shared by the method and the entry point.
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
            let text = writer.finish();
            with_current(|context| context.intern_value(text).bits())
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
    //
    // Parse an existing string through a borrow of its heap text — the
    // fast path, and the common one. `super::text::to_text` handled the
    // rest, but it is the PRIMITIVE half of `ToString`: it answers `None`
    // for an object rather than running one, so `JSON.parse([1])` — whose
    // conversion is `Array.prototype.join`, called through `ToPrimitive`
    // — silently became `undefined`. `to_string_value` is the full
    // conversion, called OUTSIDE the borrow because that is user code.
    //
    // PARSED INSIDE THE BORROW, where it was cloned out of it first. The
    // clone was the whole document, copied so that a reader which touches
    // no context could run outside one — and `read` names no runtime type
    // at all, which is exactly what makes running it inside safe.
    let read_at = |value: u64| {
        with_current(|context| {
            Value(value)
                .as_slot()
                .and_then(|cell| context.text_at(cell))
                .map(read::parse_text)
        })
    };
    let parsed = match read_at(text) {
        Some(parsed) => Some(parsed),
        None => match super::text::to_string_value(text) {
            Some(value) => read_at(value),
            // Either the conversion raised (a symbol, or a `toString`
            // that threw) and rule 8's caller re-raises, or it did not
            // and there is nothing further to try — both read the same
            // here.
            None => None,
        },
    };
    let Some(parsed) = parsed else {
        return with_current(|context| undefined_of(context));
    };
    match parsed {
        Some(node) => {
            let value = materialise(node, 0, false, &mut Vec::new());
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

/// The keys the previous object at each depth had.
///
/// A document is mostly rows, and the rows of a document spell the same keys in
/// the same order. Interning one is a hash of its text and two table lookups,
/// paid per member per row for an answer that was given one row earlier — so
/// the last object at each depth leaves its keys behind, position by position,
/// and the next one compares text (a `memcmp` of a few bytes) before it hashes.
/// By depth because a row's own nested objects would otherwise overwrite the
/// row's keys between one row and the next. Numbers, never references: nothing
/// here is something the collector needs to hear about.
///
/// And with the keys, the LAYOUT they arrived at: a row whose every key matched
/// is the same shape as the row before it, so it is typed and filled directly
/// rather than walking a transition per member to rediscover that.
type Seen = Vec<Row>;

/// What the last object at one depth left behind.
#[derive(Default)]
struct Row {
    keys: Vec<(Str, rts_cranelift::shape::Key)>,
    /// Where `keys`, in this order, put their values — cleared the moment
    /// `keys` changes, because it is a fact about exactly that sequence.
    laid: Option<super::clone::Laid>,
}

/// The heap value a parsed node names.
///
/// Every allocation in this module is reached from here, and every one of them
/// happens with no borrow held above it: a composite builds its children first,
/// each through its own borrows, and only then takes the one that stores them.
/// That ordering is the whole reason the parser answers a tree.
///
/// `seen` is [`Seen`]: what lets a row skip interning its keys.
fn materialise(node: read::Node, depth: usize, row: bool, seen: &mut Seen) -> u64 {
    match node {
        Node::Null => with_current(|context| Value::from_singleton(context.singletons.null).bits()),
        Node::Bool(flag) => Value::from_bool(flag).bits(),
        Node::Number(number) => Value::from_f64(number).bits(),
        // MOVED in: the reader built this `Str` for exactly this cell, and a
        // clone here was a second copy of every string in the document.
        Node::Text(text) => with_current(|context| context.intern_value(text).bits()),
        Node::Array(items) => {
            // ROOTED, and a loop rather than a `collect`: `materialise` is
            // recursive and every branch of it ALLOCATES, so the children built
            // so far are exposed between the steps of the loop that makes them
            // — named only by a `Vec` on the Rust heap, which no scan of ours
            // reaches. See `super::rooted`.
            let mut built = super::rooted::Rooted::new();
            for item in items {
                let value = materialise(item, depth + 1, true, seen);
                built.values().push(value);
            }
            // Allocate the array while the child values remain registered;
            // only then transfer them into the array side table.
            with_current(|context| super::array::built_in_rooted(context, built))
        }
        Node::Object(members) => {
            // The same, with keys kept beside the guard: `Rooted` holds values,
            // and a `Str` is not one — it is text this function has not interned
            // yet, on the Rust heap where nothing can collect it.
            let mut values = super::rooted::Rooted::new();
            let mut built: Vec<(Str, u64)> = Vec::with_capacity(members.len());
            for (key, value) in members {
                let made = materialise(value, depth + 1, false, seen);
                values.values().push(made);
                built.push((key, made));
            }
            // The guard stays ALIVE past this line: `native::plain`, interning
            // keys and every shape transition below may allocate, so the values
            // remain registered until they land in the new object.
            let made = with_current(|context| {
                let Some(cell) = super::native::plain(context) else {
                    return undefined_of(context);
                };
                // THE CELL ITSELF IS A ROOT FROM HERE, and it was not.
                //
                // `cell` is a bare `u32` in a Rust frame. The stack scan
                // recognises an encoded `Value`, and a raw index is not one — so
                // between this line and the last store the object being built was
                // named by nothing the collector walks. The fallback arm below
                // calls `put` once per member, and a `put` that grows the spill
                // reaches `alloc_or_die`, which may collect; the cell was then
                // freed and handed out again while the loop went on writing into
                // it.
                //
                // **It is a SILENT WRONG ANSWER before it is a crash, and this
                // comment said the opposite.** Measured against a kept pre-fix
                // binary: a TWENTY-key object parsed 60 000 times comes back
                // twice with `Object.keys(o).length === 0` and the process exits
                // ZERO. The segfault the first version of this comment described
                // is what happens further along, once enough recycled cells have
                // been written through. So the reader of an earlier draft would
                // have concluded that a small object is safe and that a clean
                // exit means a clean parse; neither is true.
                //
                // The threshold is the SPILL, not eighty keys: the exposed
                // allocation is `spill_set` -> `alloc_spanning_or_die`, which
                // both arms reach. The fast arm bounds at `region.width_of`
                // (fifteen) while `set_slot_value` subtracts `owned_slots`
                // (fourteen), so the fifteenth property spills with `fallback`
                // still false.
                //
                // An object grown the ordinary way never had this: `const o = {};
                // o.k = v` holds the object in a machine slot as an encoded
                // value, which the scan does see. Only a native building a cell
                // out of a Rust local is exposed, and this is the one that builds
                // a wide one.
                values.values().push(Value::from_slot(cell).bits());
                // The layout is reached ONCE — `clone::populate`, which this
                // wrote first and which the clone and the pickle now share.
                //
                // Interned as a NAME, never as an index, which is what
                // `computed::property_key` does for every computed key — so
                // `JSON.parse("{\"0\":1}")[0]` finds what was stored. Routing
                // `"0"` through `Key::from_str` would file it among the
                // elements of an object that has none.
                // Only a ROW — an object that is an array's element — consults
                // or leaves a memo. An object reached any other way has no
                // sibling to repeat for, and the memo then costs a `Vec` per
                // depth and a moved key per member to save nothing: measured,
                // five nested objects went from 2 036 ns to 4 524 with it on.
                if !row {
                    let members: Vec<(crate::object::Key, u64)> = built
                        .iter()
                        .map(|(key, value)| {
                            let named = context.interner.intern(key, &mut context.keys);
                            (crate::object::Key::Name(named), *value)
                        })
                        .collect();
                    super::clone::populate(context, cell, &members);
                    return Value::from_slot(cell).bits();
                }
                if seen.len() <= depth {
                    seen.resize_with(depth + 1, Row::default);
                }
                let before = &mut seen[depth];
                let mut same = before.keys.len() == built.len();
                let members: Vec<(crate::object::Key, u64)> = built
                    .into_iter()
                    .enumerate()
                    .map(|(at, (key, value))| {
                        let named = match before.keys.get(at) {
                            Some((text, named)) if *text == key => *named,
                            _ => {
                                let named = context.interner.intern(&key, &mut context.keys);
                                same = false;
                                before.laid = None;
                                before.keys.truncate(at);
                                before.keys.push((key, named));
                                named
                            }
                        };
                        (crate::object::Key::Name(named), value)
                    })
                    .collect();
                let repeated = same
                    && before.laid.as_ref().is_some_and(|laid| {
                        super::clone::populate_as(context, cell, laid, members.iter().map(|(_, value)| *value))
                    });
                if !repeated {
                    let laid = super::clone::populate_laid(context, cell, &members);
                    if before.keys.len() == members.len() {
                        before.laid = laid;
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
