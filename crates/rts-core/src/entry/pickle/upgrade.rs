//! Schema versions: a class says which shape of its fields it expects, and
//! migrates older ones itself.
//!
//! ```text
//! import { version, upgrade } from "rts:serde";
//! class Save {
//!   static [version] = 2;
//!   static [upgrade](fields, from) { if (from < 2) fields.gold = fields.coins * 10; return fields; }
//! }
//! ```
//!
//! The writer puts the class's `version` (0 when it declares none) in the
//! stream beside its name. The reader compares it with the version the SAME
//! class declares in the program reading, and when the two differ and the
//! class declares `upgrade`, calls it with the stream's fields and the version
//! they were written under — BEFORE the instance is revived. What it answers
//! (or the object it was handed, if it answers nothing) is what the instance
//! gets. Without `upgrade`, or with equal versions, fields are matched by key
//! exactly as they would have been: this is opt-in.
//!
//! # Why this does not break "decoding runs no code from the stream"
//!
//! `upgrade` is code the DESTINATION program declared, on a class it declared,
//! found by the name the stream gave — the same lookup that finds the class's
//! prototype. The stream chooses which declared class it names and which
//! version number it claims; it cannot supply a function, and it cannot make a
//! class run anything that class did not already say it would run on
//! migration. That is the line Python's pickle crosses with `__reduce__`, whose
//! callable comes from the stream; this one does not.
//!
//! # Private fields
//!
//! The fields object spells the class's OWN `#private` fields as `"#name"`,
//! which is how a program writes one, and an answer spelling `"#name"` writes
//! the private field back. A private field an ANCESTOR class declared is not
//! shown — it is that class's, not this one's to migrate — and is carried
//! through to the instance unchanged. A PUBLIC field a program named with a
//! leading `#` (`o["#x"] = 1`) is read as the private one here; stated because
//! it is the one collision this spelling has.
//!
//! # When it runs
//!
//! Every object of the stream exists before the first `upgrade` is called, so
//! the fields it is handed may point anywhere in the graph — including at
//! instances not yet upgraded, which are empty until their own turn. An
//! `upgrade` that reads another instance's fields sees them only if that
//! instance came first in the stream.

use super::super::clone::{Graph, Made, Node, populate, resolve};
use super::super::objects::undefined_of;
use super::super::rooted::Rooted;
use super::super::{Context, with_current};
use super::Failure;
use crate::object::Key;
use crate::value::Value;

/// One instance whose class migrates it.
pub(super) struct Pending {
    at: usize,
    constructor: u64,
    upgrade: u64,
    from: u64,
}

impl Pending {
    /// The node it is.
    pub(super) fn at(&self) -> usize {
        self.at
    }
}

/// The instances of a decoded arena whose class declares `upgrade` and a
/// version other than the one they were written under — sorted, which is what
/// `materialise_holding` asks of the list it skips.
pub(super) fn pending(context: &mut Context, graph: &Graph) -> Vec<Pending> {
    let mut found = Vec::new();
    for (at, node) in graph.nodes.iter().enumerate() {
        let Node::Instance { class, .. } = node else {
            continue;
        };
        let Some(prototype) = Value(class.prototype).as_slot() else {
            continue;
        };
        let key = context.well_known("constructor");
        let Some(constructor) = super::super::objects::own_property(context, prototype, key).and_then(|found| found.as_slot())
        else {
            continue;
        };
        if super::names::version_of(context, constructor) == class.version {
            continue;
        }
        let key = context.well_known(super::names::UPGRADE);
        let upgrade = super::super::objects::own_property(context, constructor, key)
            .filter(|found| found.as_slot().is_some_and(|cell| context.callable_at(cell).is_some()));
        if let Some(upgrade) = upgrade {
            found.push(Pending {
                at,
                constructor: Value::from_slot(constructor).bits(),
                upgrade: upgrade.bits(),
                from: class.version,
            });
        }
    }
    found
}

/// Runs each `upgrade` and fills its instance with what it answered, then
/// answers the root.
///
/// Outside any borrow — each call is user code — and with `made` alive the
/// whole time: it is what keeps every object of the graph reachable while an
/// `upgrade` allocates.
pub(super) fn run(graph: &Graph, made: Made, pending: Vec<Pending>, root: super::super::clone::Slot) -> Result<u64, Failure> {
    for each in pending {
        let (fields, carried, own, absent) = with_current(|context| {
            let (fields, carried, own) = handed(context, graph, &made, each.at);
            (fields, carried, own, undefined_of(context))
        });
        // Rooted across the call: nothing else names the object yet.
        let mut held = Rooted::with(vec![fields]);
        let from = Value::from_f64(each.from as f64).bits();
        let answered = super::super::functions::call(each.upgrade, each.constructor, fields, from, absent, absent);
        // Rule 8 of this crate's README: ask before believing the answer.
        if super::super::throw::in_flight() {
            return Err(Failure::Thrown);
        }
        held.values().push(answered);
        with_current(|context| {
            let source = match Value(answered).as_slot().is_some_and(|cell| !context.is_text_at(cell)) {
                true => answered,
                false => fields,
            };
            let instance = resolve(super::super::clone::Slot::At(each.at), &made);
            if let (Some(cell), Some(source_cell)) = (Value(instance).as_slot(), Value(source).as_slot()) {
                let mut members = read_back(context, source, source_cell, own);
                members.extend(carried);
                populate(context, cell, &members);
            }
        });
    }
    Ok(resolve(root, &made))
}

/// The plain object an `upgrade` is handed — the stream's public fields and
/// the class's own private ones spelled `"#name"` — with the ancestors'
/// private fields set aside to be carried through, and the class's own
/// private-name number.
fn handed(context: &mut Context, graph: &Graph, made: &Made, at: usize) -> (u64, Vec<(Key, u64)>, Option<u32>) {
    let Node::Instance { class, fields } = &graph.nodes[at] else {
        return (undefined_of(context), Vec::new(), None);
    };
    let own = super::names::own_space(context, class.prototype);
    let mut shown = Vec::with_capacity(fields.len());
    let mut carried = Vec::new();
    for (key, slot) in fields {
        let held = resolve(*slot, made);
        match super::names::private_parts(context, *key) {
            Some((space, name)) if own.is_some_and(|own| own.to_string() == space) => {
                let key = Key::Name(context.interner.intern_str(&format!("#{name}"), &mut context.keys));
                shown.push((key, held));
            }
            Some(_) => carried.push((*key, held)),
            None => shown.push((*key, held)),
        }
    }
    let Some(cell) = super::super::native::plain(context) else {
        return (undefined_of(context), carried, own);
    };
    // Held while the layout is reached: `populate` may grow a spill, and the
    // cell is only a Rust local until the caller roots the answer.
    let _held = Rooted::with(vec![Value::from_slot(cell).bits()]);
    populate(context, cell, &shown);
    (Value::from_slot(cell).bits(), carried, own)
}

/// What an `upgrade` answered, as members for the instance — its own
/// enumerable data properties, `"#name"` written back as the class's private
/// field.
fn read_back(context: &mut Context, source: u64, cell: u32, own: Option<u32>) -> Vec<(Key, u64)> {
    super::super::clone::data_members(context, source, cell)
        .into_iter()
        .map(|(key, held)| match own {
            Some(own) => (respelled(context, key, "#", &format!("@@#{own}#")), held),
            None => (key, held),
        })
        .collect()
}

/// A key with one prefix swapped for another, or the key unchanged.
fn respelled(context: &mut Context, key: Key, from: &str, to: &str) -> Key {
    let Key::Name(named) = key else {
        return key;
    };
    let Some(text) = context.interner.text(named).and_then(|text| text.to_rust()) else {
        return key;
    };
    match text.strip_prefix(from) {
        Some(rest) => Key::Name(context.interner.intern_str(&format!("{to}{rest}"), &mut context.keys)),
        None => key,
    }
}
