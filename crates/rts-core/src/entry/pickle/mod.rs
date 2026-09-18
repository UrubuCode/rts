//! `rts:serde` — the pickle: a value graph as bytes, and back.
//!
//! `docs/engine/pickle.md` is the specification — the stream, what each kind
//! becomes, the limits, and what it costs. This is the implementation, and its
//! modules split where the work does:
//!
//! | module | what it holds |
//! |---|---|
//! | `format` | opcodes, varints, WTF-8, the cursor — bytes only |
//! | `write` | an arena to bytes |
//! | `read` | bytes to an arena, v1 and v2 |
//! | `strings` | the stream's table of strings, read side |
//! | `legacy` | what v1 wrote that v2 does not, as what it meant |
//! | `names` | which classes and functions a stream may name |
//! | `kinds` | a typed array's kind as a byte |
//!
//! # What it reuses, and why that is the design rather than a shortcut
//!
//! The walk from a value to an arena is `structuredClone`'s — `super::clone` —
//! and so is the build from an arena to values. A pickle is exactly those two
//! halves with bytes between them, so writing a second walk would have been a
//! second answer to "what is this value", and the two would have disagreed the
//! first time a kind was added to one. The walk grew a [`Policy`] instead: the
//! classification is shared, and what each reader does with a class instance,
//! a function or a refusal is a `match` on it.
//!
//! # Where the borrows are
//!
//! `serialize` walks — one borrow for a graph of plain data, more only where a
//! getter must run — and then writes the stream and copies it into a
//! `Uint8Array` inside one more. `deserialize` is ONE borrow: the input's bytes
//! are read where they are, the arena is built, and the arena is materialised,
//! with no call into user code anywhere in it.

mod format;
mod kinds;
mod legacy;
pub(in crate::entry) mod names;
mod read;
mod strings;
mod write;

use super::clone::{Policy, Refusal, materialise, resolve, walk};
use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

/// Why a pickle could not be made or read, as a host sees it.
#[derive(Debug)]
pub enum Failure {
    /// What the program should be told, as a `TypeError`.
    Refused(String),
    /// A getter the walk ran threw; the error is already in flight, and the
    /// compiled call site above re-raises it.
    Thrown,
}

/// The bytes a value pickles to — for a host surface built on the pickle,
/// `node:v8` being the one.
///
/// Ambient: it takes its own borrows, because the walk may run a getter, and so
/// must not be called from inside `with_runtime`.
pub fn pickle_value(value: u64) -> Result<Vec<u8>, Failure> {
    let (graph, root) = walk(Policy::Pickle, value).map_err(failure)?;
    with_current(|context| write::write(context, &graph, root)).map_err(Failure::Refused)
}

/// The value a stream describes, built on the heap.
///
/// Inside the caller's borrow: reading runs no user code, so nothing here ever
/// needs to give it back.
pub fn unpickle_bytes(context: &mut Context, bytes: &[u8]) -> Result<u64, Failure> {
    let (graph, root) = read::read(context, bytes).map_err(Failure::Refused)?;
    let made = materialise(context, &graph);
    Ok(resolve(root, &made))
}

/// A list of strings as the stream of the array holding them.
///
/// For a host whose state is Rust text rather than a value graph — `Storage`
/// is the one — so that what it writes is a pickle a program can read back with
/// `deserialize`, without the host building JavaScript strings it would then
/// have to keep alive across the allocations of the ones after them.
pub fn pickle_texts(texts: &[&str]) -> Vec<u8> {
    let mut graph = super::clone::Graph::default();
    let elements = texts
        .iter()
        .map(|text| graph.text(crate::text::Str::from_str(text)))
        .collect();
    let root = super::clone::Slot::At(graph.push(super::clone::Node::Array { elements, extra: Vec::new() }));
    // A graph of text and one array has nothing a writer can refuse.
    with_current(|context| write::write(context, &graph, root)).unwrap_or_default()
}

/// The strings of a stream [`pickle_texts`] wrote — or of any stream whose
/// value is an array of strings — or `None` for anything else.
///
/// Read into the arena only: nothing is built on the heap, so nothing can be
/// collected out from under the answer.
pub fn texts_of(context: &mut Context, bytes: &[u8]) -> Option<Vec<String>> {
    let (graph, root) = read::read(context, bytes).ok()?;
    let super::clone::Slot::At(at) = root else {
        return None;
    };
    let super::clone::Node::Array { elements, .. } = graph.nodes.get(at)? else {
        return None;
    };
    elements
        .iter()
        .map(|slot| match slot {
            super::clone::Slot::Text(text) => graph.texts.get(*text)?.to_rust(),
            _ => None,
        })
        .collect()
}

fn failure(refusal: Refusal) -> Failure {
    match refusal {
        Refusal::Unserializable(message) => Failure::Refused(message),
        Refusal::Thrown => Failure::Thrown,
    }
}

/// `import { serialize, deserialize } from "rts:serde"`.
///
/// A namespace rather than globals: the pickle is this engine's own surface,
/// which is what an `rts:` specifier is for — no other runtime has a global
/// `serialize`, and inventing one would be a name a program written for
/// another runtime could collide with.
#[rtse::class("serde", namespace)]
impl Serde {
    /// `serialize(value)` — the value as RTSP bytes, a `Uint8Array`.
    ///
    /// Cycles and shared references survive; so do class instances (private
    /// fields included) and top-level functions, by name. What cannot be
    /// written — a symbol, a proxy, a closure, a promise — is a `TypeError`
    /// naming it.
    fn serialize(value: u64) -> u64 {
        let (graph, root) = match walk(Policy::Pickle, value) {
            Ok(walked) => walked,
            Err(refusal) => return raised(failure(refusal)),
        };
        let written = with_current(|context| {
            write::write(context, &graph, root).map(|bytes| super::modules::make_bytes(context, &bytes))
        });
        match written {
            Ok(made) => made,
            Err(message) => raised(Failure::Refused(message)),
        }
    }

    /// `deserialize(bytes)` — the value a stream describes.
    ///
    /// Takes a `Uint8Array`, a `Buffer`, an `ArrayBuffer`, or the `number[]`
    /// v1 answered. Runs no code from the stream: no constructor, no getter, no
    /// setter — a class is re-linked to the prototype this program declared,
    /// and a function is looked up by name among the ones it declared.
    fn deserialize(bytes: u64) -> u64 {
        let answered = with_current(|context| match input(context, bytes) {
            Some(input) => unpickle_bytes(context, &input),
            None => Err(Failure::Refused(
                "pickle: deserialize takes a Uint8Array, a Buffer, an ArrayBuffer or an array of bytes".into(),
            )),
        });
        match answered {
            Ok(value) => value,
            Err(failed) => raised(failed),
        }
    }
}

/// The module object `rts:serde` names.
pub fn namespace(context: &mut Context) -> u64 {
    register_serde(context)
}

/// Raises a failure as the `TypeError` it is, outside any borrow, and answers
/// what the call site discards.
fn raised(failed: Failure) -> u64 {
    if let Failure::Refused(message) = failed {
        super::throw::type_error(&message);
    }
    with_current(|context| undefined_of(context))
}

/// The bytes of whatever `deserialize` was handed.
///
/// A copy, and one: the arena is built from a slice while the context is
/// borrowed mutably, and the input lives in that context. The copy is a
/// `memcpy` against a decode that touches every byte anyway.
fn input(context: &Context, value: u64) -> Option<Vec<u8>> {
    if let Some(bytes) = super::modules::buffer_source_bytes(context, value) {
        return Some(bytes);
    }
    let cell = Value(value).as_slot()?;
    let elements = context.elements_at(cell)?;
    elements
        .iter()
        .map(|element| {
            let number = Value(*element).numeric()?;
            (number.fract() == 0.0 && (0.0..=255.0).contains(&number)).then_some(number as u8)
        })
        .collect()
}
