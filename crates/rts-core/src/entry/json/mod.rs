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
use super::rooted::Rooted;
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
    let kept = with_current(|context| std::mem::take(&mut context.json.kept));
    let mut writer = write::Writer::new(write::indent_of(space), replacer, kept);
    let root_key = with_current(|context| context.well_known_text(""));
    let value = writer.hooked(holder, value, super::json::write::HookKey::Given(root_key));
    match writer.write(value, 0) && !super::throw::in_flight() {
        true => {
            let (text, kept) = writer.finish();
            with_current(|context| {
                context.json.keep(kept);
                context.intern_value(text).bits()
            })
        }
        false => {
            let kept = writer.abandon();
            with_current(|context| {
                context.json.keep(kept);
                undefined_of(context)
            })
        }
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
            let value = materialise_document(node);
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

/// What one call leaves in the [`super::Context`] for the next.
///
/// # Why anything outlives a call
///
/// Because a program serialises the same shapes again and again, and a call
/// that starts from nothing pays to learn them every time: the key list of a
/// shape is a walk up the shape tree into a fresh `Vec`, then per key an
/// attribute lookup, the interner's text and two scans of it; the path and the
/// output are two more allocations; and `parse` hashes every key of every
/// document to be told the number it was told last time. Measured 2026-09-19,
/// `target/release/rts.exe`: an EMPTY object cost 190 ns more to serialise than
/// a number did.
///
/// # Why it is safe to keep
///
/// Nothing here is a reference. A plan is a shape's number, its keys' numbers,
/// slots and label bytes; a shape never changes and its number is never handed
/// out twice, so a plan cannot go stale — only unused. A row is key text and
/// key numbers, and a layout is a type's number. So the collector has nothing
/// to hear about (rule 10), and what bounds it is the DEPTH of the last
/// document and the keys at each depth, with [`Scratch::keep`] refusing a buffer
/// past a megabyte and [`KEY_LIMIT`] refusing a key nobody will repeat.
///
/// Taken with `mem::take` and given back, as `collect_cycle::sweep` does with
/// `doomed`: a `toJSON` that serialises re-enters with an empty one and the
/// outer call's is what is kept.
#[derive(Default)]
pub(in crate::entry) struct Scratch {
    kept: write::Kept,
    seen: Seen,
    /// The stack `materialise` builds on, kept EMPTY and for its capacity: a
    /// fresh one grows by doubling, which for a sixteen-element array is three
    /// reallocations to hold what the last document already made room for.
    stack: Vec<u64>,
}

impl Scratch {
    fn keep(&mut self, mut kept: write::Kept) {
        if kept.2.capacity() > 1 << 20 {
            kept.2 = Vec::new();
        }
        self.kept = kept;
    }
}

/// The longest key a [`Row`] remembers. A longer one is data wearing a key's
/// position — a hash, a path — and keeping it would hold its text for nothing.
const KEY_LIMIT: usize = 64;

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
fn materialise(node: read::Node, depth: usize, seen: &mut Seen, built: &mut Rooted) -> u64 {
    let node = match scalar(node) {
        Ok(value) => return value,
        Err(composite) => composite,
    };
    match node {
        // Unreachable: `scalar` answered every node that is not a composite.
        // Answered rather than asserted, because a panic under an `extern "C"`
        // frame is an abort.
        Node::Null | Node::Bool(_) | Node::Number(_) | Node::Text(_) => {
            with_current(|context| undefined_of(context))
        }
        Node::Array(items) => {
            let from = built.len();
            for item in items {
                let value = materialise(item, depth + 1, seen, built);
                built.values().push(value);
            }
            let made = with_current(|context| super::array::built_in_from(context, &built.as_slice()[from..]));
            built.values().truncate(from);
            made
        }
        Node::Object(members) => {
            // THE KEYS FIRST, and that order is what makes the rest cheap. A key
            // is a number and resolving one allocates no cell, so it can happen
            // before any child exists — and it settles, before a single value is
            // built, whether this object is the row before it over again.
            let count = members.len();
            let fresh = with_current(|context| row_keys(context, &members, seen_at(seen, depth)));
            let from = built.len();
            for (_, value) in members {
                let made = materialise(value, depth + 1, seen, built);
                built.values().push(made);
            }
            let made = with_current(|context| {
                let Some(cell) = super::native::plain(context) else {
                    return undefined_of(context);
                };
                // THE CELL ITSELF IS A ROOT FROM HERE, and it was not.
                //
                // `cell` is a bare `u32` in a Rust frame. The stack scan
                // recognises an encoded `Value`, and a raw index is not one — so
                // between this line and the last store the object being built was
                // named by nothing the collector walks, and a `put` that grows
                // the spill may collect. **It is a SILENT WRONG ANSWER before it
                // is a crash**: measured against a kept pre-fix binary, a
                // twenty-key object parsed 60 000 times came back twice with no
                // keys and the process exited zero. `docs/engine/lost-roots.md`.
                built.values().push(Value::from_slot(cell).bits());
                let values = &built.as_slice()[from..from + count];
                let row = seen_at(seen, depth);
                let repeated = fresh.is_none()
                    && row.laid.as_ref().is_some_and(|laid| {
                        super::clone::populate_as(context, cell, laid, values.iter().copied())
                    });
                if !repeated {
                    // Interned as a NAME, never as an index, which is what
                    // `computed::property_key` does for every computed key — so
                    // an object keyed `"0"` is read back by `[0]`.
                    let keys = fresh
                        .unwrap_or_else(|| row.keys.iter().map(|(_, named)| *named).collect());
                    let members: Vec<(crate::object::Key, u64)> = keys
                        .iter()
                        .zip(values)
                        .map(|(named, value)| (crate::object::Key::Name(*named), *value))
                        .collect();
                    let laid = super::clone::populate_laid(context, cell, &members);
                    if row.keys.len() == count {
                        row.laid = laid;
                    }
                }
                Value::from_slot(cell).bits()
            });
            built.values().truncate(from);
            made
        }
    }
}

/// A node that is one value, or the node back when it is a composite.
///
/// Stated once for the two places that ask: a document that is a scalar, and a
/// scalar inside one.
fn scalar(node: read::Node) -> Result<u64, read::Node> {
    Ok(match node {
        Node::Null => with_current(|context| Value::from_singleton(context.singletons.null).bits()),
        Node::Bool(flag) => Value::from_bool(flag).bits(),
        Node::Number(number) => Value::from_f64(number).bits(),
        // MOVED in: the reader built this `Str` for exactly this cell, and a
        // clone here was a second copy of every string in the document.
        Node::Text(text) => with_current(|context| context.intern_value(text).bits()),
        composite => return Err(composite),
    })
}

/// A whole document. A scalar needs neither the rooted stack nor the rows, and
/// taking them cost it more than it cost to build: `JSON.parse("42")` went
/// from 58 ns to 92 the day every parse set both up.
fn materialise_document(node: read::Node) -> u64 {
    let node = match scalar(node) {
        Ok(value) => return value,
        Err(composite) => composite,
    };
    let (mut seen, stack) = with_current(|context| {
        (std::mem::take(&mut context.json.seen), std::mem::take(&mut context.json.stack))
    });
    let mut built = Rooted::with(stack);
    let value = materialise(node, 0, &mut seen, &mut built);
    // Un-registered EMPTY: every composite truncated back to where it started,
    // so there is nothing in it left to keep alive — and `value` is an encoded
    // word in this frame, which the stack scan reads.
    let mut stack = built.take();
    stack.clear();
    with_current(|context| {
        context.json.seen = seen;
        if stack.capacity() <= 1 << 16 {
            context.json.stack = stack;
        }
    });
    value
}

/// The row a depth remembers, made if this is the first object that deep.
fn seen_at(seen: &mut Seen, depth: usize) -> &mut Row {
    if seen.len() <= depth {
        seen.resize_with(depth + 1, Row::default);
    }
    &mut seen[depth]
}

/// Resolves an object's keys against the row before it.
///
/// `None` when every key, in order and in number, is that row's — the caller
/// then needs no list at all. Otherwise this object's keys, with the row
/// rewritten to be them, so the NEXT object can be the one that matches.
fn row_keys(
    context: &mut super::Context,
    members: &[(Str, Node)],
    row: &mut Row,
) -> Option<Vec<rts_cranelift::shape::Key>> {
    let matches = row.keys.len() == members.len()
        && row.keys.iter().zip(members).all(|((text, _), (key, _))| text == key);
    if matches {
        return None;
    }
    row.laid = None;
    let mut keys = Vec::with_capacity(members.len());
    let mut kept = true;
    for (at, (key, _)) in members.iter().enumerate() {
        let named = match row.keys.get(at) {
            Some((text, named)) if kept && text == key => *named,
            _ => {
                let named = context.interner.intern(key, &mut context.keys);
                if kept {
                    row.keys.truncate(at);
                }
                // A key past the limit ends what this row remembers: the text
                // after it would be filed at the wrong position.
                kept = kept && key.len() <= KEY_LIMIT;
                if kept {
                    row.keys.push((key.clone(), named));
                }
                named
            }
        };
        keys.push(named);
    }
    Some(keys)
}
