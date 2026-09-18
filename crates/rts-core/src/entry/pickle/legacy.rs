//! What v1 wrote that v2 does not, turned into what it meant.
//!
//! # Why v1 needs translating at all
//!
//! v1 was the OLD engine's format, and it wrote the old engine's internals.
//! Its `Map` was a class with `#keys`/`#vals` and a hash index `#h`/`#nx`/
//! `#mask`; its `Set` the same with `#items`; its `Error` an instance with
//! `message`/`name`/`stack`/`cause` fields. None of those classes exists here,
//! so a v1 stream naming one is read as the thing it stood for: a real `Map`, a
//! real `Set`, a real error. The hash index is ignored — it indexed the old
//! engine's memory, and the entries in insertion order are the whole state.
//!
//! Everything else in v1 is read by the same code v2 uses, which is the reason
//! the `Date` and `RegExp` payloads were kept as they were.

use super::super::clone::{ClassName, ErrorClass, Node, Slot};
use super::super::Context;
use super::format::Broken;
use super::read::Reader;
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// The error classes a stream may name, as the static text a node carries.
const ERRORS: [&str; 8] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
];

/// The standard error class a name spells, or `Error` for one it does not.
pub(super) fn error_class(name: &Str) -> &'static str {
    let spelled = name.to_rust_lossy();
    ERRORS.into_iter().find(|known| *known == spelled).unwrap_or("Error")
}

/// An `OP_EXT` payload: a `Date` or a `RegExp`, the two v1 gave a codec.
pub(super) fn extension(tag: &[u8], payload: &[u8]) -> Result<Node, Broken> {
    match tag {
        b"Date" => {
            let word = payload.get(..8).ok_or("pickle: a Date payload shorter than 8 bytes")?;
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(word);
            let ms = i64::from_le_bytes(bytes);
            Ok(Node::Date(match ms {
                i64::MIN => f64::NAN,
                _ => ms as f64,
            }))
        }
        b"RegExp" => {
            let mut cursor = super::format::Cursor::new(payload);
            let source = utf8_field(&mut cursor)?;
            let flags = utf8_field(&mut cursor)?;
            let last_index = (cursor.u64()? as i64).max(0) as f64;
            Ok(Node::Regexp { source, flags, last_index })
        }
        other => Err(format!(
            "pickle: no reader for the extension '{}'",
            String::from_utf8_lossy(other)
        )),
    }
}

/// A `u32`-prefixed UTF-8 field of the `RegExp` payload.
fn utf8_field(cursor: &mut super::format::Cursor) -> Result<String, Broken> {
    let mut word = [0u8; 4];
    word.copy_from_slice(cursor.take(4)?);
    let length = u32::from_le_bytes(word) as usize;
    String::from_utf8(cursor.take(length)?.to_vec()).map_err(|_| "pickle: a RegExp text that is not UTF-8".into())
}

/// A v1 class instance, as the thing it stood for.
pub(super) fn class(reader: &mut Reader, name: Str, keys: Vec<Str>, values: Vec<Slot>) -> Result<Node, Broken> {
    let field = |wanted: &str| {
        keys.iter()
            .position(|key| key.to_rust_lossy() == wanted)
            .map(|at| values[at])
    };
    let spelled = name.to_rust_lossy();
    match spelled.as_str() {
        "Map" => {
            let keys = elements(reader, field("#keys"))?;
            let held = elements(reader, field("#vals"))?;
            if keys.len() != held.len() {
                return Err("pickle: a v1 Map whose keys and values differ in count".into());
            }
            Ok(Node::Map(keys.into_iter().zip(held).collect()))
        }
        "Set" => Ok(Node::Set(elements(reader, field("#items"))?)),
        _ if ERRORS.contains(&spelled.as_str()) => {
            let undefined = Value::from_singleton(reader.context.singletons.undefined).bits();
            let present = |slot: Option<Slot>| slot.filter(|slot| *slot != Slot::Bits(undefined));
            Ok(Node::Error {
                class: ErrorClass::Builtin(error_class(&name)),
                message: present(field("message")),
                stack: present(field("stack")),
                cause: present(field("cause")),
                extra: Vec::new(),
            })
        }
        _ => {
            let found = super::names::resolve(reader.context, None, &name)?;
            let prototype = prototype_of(reader.context, found, &name)?;
            let mut interned: Vec<Key> = keys
                .iter()
                .map(|key| Key::Name(reader.context.interner.intern(key, &mut reader.context.keys)))
                .collect();
            let spaces = Value(prototype)
                .as_slot()
                .map(|prototype| super::names::spaces(reader.context, prototype))
                .unwrap_or_default();
            super::names::local(reader.context, &spaces, &mut interned, true);
            let fields = interned.into_iter().zip(values).collect();
            let class = ClassName {
                module: std::rc::Rc::new(Str::empty()),
                name: std::rc::Rc::new(name),
                prototype,
                version: 0,
            };
            Ok(Node::Instance { class, fields })
        }
    }
}

/// The elements of an array a v1 field named, read out of the arena.
fn elements(reader: &Reader, slot: Option<Slot>) -> Result<Vec<Slot>, Broken> {
    match slot {
        Some(Slot::At(at)) => match reader.graph.nodes.get(at) {
            Some(Node::Array { elements, .. }) => Ok(elements.clone()),
            _ => Err("pickle: a v1 collection whose entries are not an array".into()),
        },
        _ => Err("pickle: a v1 collection with its entries missing".into()),
    }
}

/// What instances of a declared class inherit from: its `prototype`.
pub(super) fn prototype_of(context: &mut Context, found: u64, name: &Str) -> Result<u64, Broken> {
    let prototype = Value(found).as_slot().and_then(|cell| {
        let key = context.well_known("prototype");
        super::super::objects::own_property(context, cell, key)
    });
    match prototype.filter(|prototype| prototype.as_slot().is_some()) {
        Some(prototype) => Ok(prototype.bits()),
        None => Err(format!(
            "pickle: '{}' is declared here but has no prototype to revive an instance with",
            name.to_rust_lossy()
        )),
    }
}

/// A v1 `OP_JSON` payload — a parsed-JSON value the old engine kept whole —
/// read by this engine's own `JSON` reader and put into the arena.
pub(super) fn json(reader: &mut Reader, payload: &[u8]) -> Result<Slot, Broken> {
    let text = super::format::text(payload).ok_or("pickle: a JSON payload that is not UTF-8")?;
    let parsed = super::super::json::read::parse_text(&text).ok_or("pickle: a JSON payload that does not parse")?;
    Ok(placed(reader, parsed))
}

/// One parsed JSON node, and its children, as arena slots.
///
/// Recursive, and bounded: the JSON reader refuses a document nested past its
/// own ceiling, so the tree this walks is never deeper than that.
fn placed(reader: &mut Reader, node: super::super::json::read::Node) -> Slot {
    use super::super::json::read::Node as Json;
    let singletons = reader.context.singletons;
    match node {
        Json::Null => Slot::Bits(Value::from_singleton(singletons.null).bits()),
        Json::Bool(flag) => Slot::Bits(Value::from_bool(flag).bits()),
        Json::Number(number) => Slot::Bits(Value::from_f64(number).bits()),
        Json::Text(text) => reader.graph.text(text),
        Json::Array(items) => {
            let elements = items.into_iter().map(|item| placed(reader, item)).collect();
            Slot::At(reader.graph.push(Node::Array { elements, extra: Vec::new() }))
        }
        Json::Object(members) => {
            let members = members
                .into_iter()
                .map(|(key, value)| {
                    let key = Key::Name(reader.context.interner.intern(&key, &mut reader.context.keys));
                    (key, placed(reader, value))
                })
                .collect();
            Slot::At(reader.graph.push(Node::Object(members)))
        }
    }
}
