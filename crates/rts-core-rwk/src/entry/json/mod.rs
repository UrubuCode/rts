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
//! # What is not implemented, and why each
//!
//! **`toJSON`.** A value's own serialisation hook is a *call* into user code,
//! per value, which this layer can do — `get_indexed` already runs getters the
//! same way. What it costs is the walk: every descent would have to probe for
//! the method, release, call, and restart classification on whatever came back,
//! and a hook answering the object it hangs off turns the cycle stack into the
//! only thing standing between here and an infinite walk. It is a feature with
//! a design, not an oversight, and it waits for a program that needs it.
//!
//! **`replacer`.** Declared and ignored. Both spellings of it — a function
//! called per key and an array of keys to keep — are the `toJSON` problem with
//! a second argument, and ignoring it is stated here rather than discovered.
//!
//! **`space` is implemented**, because it is the one of the three that is pure
//! formatting: it never runs anything, never changes which members are written,
//! and it is what a program printing a file for a human to read reaches for.
//! Leaving it out would have made `JSON.stringify(o, null, 2)` answer the
//! compact form silently — a wrong answer to a call people really write, where
//! the other two answer a right one that ignored a request.
//!
//! # Where this still answers instead of throwing
//!
//! A cycle is a `TypeError` in the specification and bad JSON is a
//! `SyntaxError`. Only the second is thrown here now — `parse` calls no user
//! code, so rule 8's discipline (`crates/rts-core-rwk/README.md`) is satisfied
//! trivially and `throw::syntax_error` is reachable from an entry point that
//! never held a borrow across it. The cycle case is different: `write` calls
//! BACK into user code (getters, `toJSON`) while `self.open` is live, and a
//! raise there would need every one of those call sites to check for a throw
//! before continuing the walk, which they do not yet. So a cycle still writes
//! `null` at the point it closes, and that is the narrower, still-true gap —
//! not "a throw needs a protected region", which stopped being true the day a
//! throw learned to leave one frame.

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
/// which is a stated divergence and the reason the number is generous — a
/// document two hundred deep is machine-made, and one that deep *and* meant to
/// round-trip is not a program this engine has.
pub(super) const DEPTH: usize = 200;

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
    /// `replacer` is read and ignored; see the module documentation for why it
    /// and `toJSON` are one decision rather than two.
    fn stringify(value: u64, replacer: u64, space: u64) -> u64 {
        let _ = replacer;
        let mut writer = write::Writer::new(write::indent_of(space));
        // The root's key is the empty string — the specification calls
        // `SerializeJSONProperty` with a synthetic holder `{"": value}`, which is
        // what makes `{ toJSON(key) { return key } }` answer `""` when it is the
        // whole argument to `stringify` rather than a member of something.
        let root_key = with_current(|context| context.intern_value(Str::from_str("")).bits());
        match writer.write(value, root_key, 0) {
            true => {
                let units = writer.finish();
                with_current(|context| context.intern_value(Str::from_utf16(&units)).bits())
            }
            false => with_current(|context| undefined_of(context)),
        }
    }

    /// `JSON.parse(text)`.
    ///
    /// No `reviver`, for the reason the module gives for `replacer`: it is user
    /// code called per member, and the walk that would call it is the walk this
    /// module is built to keep flat.
    fn parse(text: u64) -> u64 {
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
            Some(node) => materialise(&node),
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
            let built: Vec<u64> = items.iter().map(materialise).collect();
            let array = super::array::array_new(built.len() as i64);
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
            let built: Vec<(Str, u64)> = members
                .iter()
                .map(|(key, value)| (Str::from_utf16(key), materialise(value)))
                .collect();
            with_current(|context| {
                let Some(cell) = super::native::plain(context) else {
                    return undefined_of(context);
                };
                for (key, value) in built {
                    // Interned as a NAME, never as an index, which is what
                    // `computed::property_key` does for every computed key —
                    // so `JSON.parse("{\"0\":1}")[0]` finds what was stored.
                    // Routing `"0"` through `Key::from_str` would file it among
                    // the elements of an object that has none.
                    let key = crate::object::Key::Name(context.interner.intern(&key, &mut context.keys));
                    super::objects::put(context, cell, key, value);
                }
                Value::from_slot(cell).bits()
            })
        }
    }
}
