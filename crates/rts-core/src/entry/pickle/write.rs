//! An arena to bytes.
//!
//! # One buffer, sized once, written once
//!
//! Everything goes into one `Vec<u8>` whose capacity is estimated from the
//! arena before the first byte — the arena knows how many nodes, elements,
//! texts and buffer bytes there are, which is most of the length. The caller
//! copies it into a `Uint8Array` once. v1 answered a `number[]`: one f64 word
//! per byte, eight times the memory, and a heap write per byte to build it.
//!
//! # No recursion
//!
//! The walk of the arena is an explicit stack of the things still to write, so
//! a nesting as deep as the arena holds costs heap and not Rust stack — the
//! reader is the same shape, which is what lets [`super::format::MAX_DEPTH`]
//! be a bound on memory rather than on a stack the program cannot see.
//!
//! # Strings are written once
//!
//! Every string in the stream — a value, an object key, a class or module
//! name — goes through one table: the first occurrence is written out in full
//! and each later one is a varint index into it. An array of ten thousand
//! records with the same five keys writes each key once. Keys are found in the
//! table by their key NUMBER before their text is hashed, because a key is
//! already interned and comparing numbers is free.

use std::collections::HashMap;

use super::super::clone::{ClassName, ErrorClass, Graph, Node, Slot};
use super::super::Context;
use super::format::*;
use crate::object::Key;
use crate::text::Str;
use crate::value::{Kind as ValueKind, Value};

/// One thing still to write, in the order the stack pops them.
enum Work {
    Value(Slot),
    Key(Key),
    Count(usize),
    /// A container is finished — its depth is given back.
    Leave,
}

struct Writer<'a> {
    context: &'a Context,
    graph: &'a Graph,
    out: Vec<u8>,
    /// Text already written, to its index in the table.
    strings: HashMap<Str, u64>,
    /// The same, by key number, for the texts that are keys.
    keys: HashMap<rts_cranelift::shape::Key, u64>,
    /// The memo id each node was written under, once it has been.
    memo: Vec<Option<u64>>,
    next_memo: u64,
}

/// The stream for one arena.
pub(super) fn write(context: &Context, graph: &Graph, root: Slot) -> Result<Vec<u8>, Broken> {
    let mut writer = Writer {
        context,
        graph,
        out: Vec::with_capacity(estimate(graph)),
        strings: HashMap::new(),
        keys: HashMap::new(),
        memo: vec![None; graph.nodes.len()],
        next_memo: 0,
    };
    writer.out.extend_from_slice(&MAGIC);
    writer.out.push(VERSION);
    let mut stack = vec![Work::Value(root)];
    let mut depth = 0usize;
    while let Some(work) = stack.pop() {
        match work {
            Work::Leave => depth -= 1,
            Work::Count(count) => varint(&mut writer.out, count as u64),
            Work::Key(key) => writer.key(key)?,
            Work::Value(slot) => writer.value(slot, &mut stack, &mut depth)?,
        }
    }
    Ok(writer.out)
}

/// What the stream will roughly weigh: a byte or two per member and per
/// element, the texts and the raw bytes at their size. An underestimate costs
/// a reallocation, never a wrong answer.
fn estimate(graph: &Graph) -> usize {
    let texts: usize = graph.texts.iter().map(Str::len).sum();
    let nodes: usize = graph
        .nodes
        .iter()
        .map(|node| match node {
            Node::Array { elements, extra } => 4 + elements.len() * 2 + extra.len() * 4,
            Node::Object(members) | Node::Bare(members) | Node::Instance { fields: members, .. } => {
                4 + members.len() * 4
            }
            Node::Map(entries) => 4 + entries.len() * 4,
            Node::Set(members) => 4 + members.len() * 2,
            Node::Buffer(bytes) | Node::NodeBuffer(bytes) | Node::View { bytes, .. } => 8 + bytes.len(),
            Node::Regexp { source, flags, .. } => 32 + source.len() + flags.len(),
            _ => 16,
        })
        .sum();
    16 + texts + nodes
}

impl Writer<'_> {
    fn value(&mut self, slot: Slot, stack: &mut Vec<Work>, depth: &mut usize) -> Result<(), Broken> {
        let at = match slot {
            Slot::Bits(bits) => return self.primitive(bits),
            Slot::Text(at) => {
                self.out.push(OP_STR);
                let text = &self.graph.texts[at];
                self.string(text);
                return Ok(());
            }
            Slot::At(at) => at,
        };
        let node = &self.graph.nodes[at];
        // A function by reference is a NAME, and naming it twice resolves to
        // the same function twice — there is no identity for a memo to keep.
        if let Node::Function(name) = node {
            self.out.push(OP_FN_REF);
            self.string(&name.module);
            self.string(&name.name);
            return Ok(());
        }
        if let Some(id) = self.memo[at] {
            self.out.push(OP_REF);
            varint(&mut self.out, id);
            return Ok(());
        }
        // The id is given on the FIRST visit, before any child is written —
        // the discipline that makes a child's back-reference to its parent
        // resolvable. The reader assigns in the same pre-order.
        self.memo[at] = Some(self.next_memo);
        self.next_memo += 1;
        *depth += 1;
        if *depth > MAX_DEPTH {
            return Err(format!("pickle: nesting deeper than {MAX_DEPTH} levels"));
        }
        stack.push(Work::Leave);
        let mark = stack.len();
        match node {
            Node::Array { elements, extra } => {
                self.out.push(OP_ARRAY);
                varint(&mut self.out, elements.len() as u64);
                stack.extend(elements.iter().map(|slot| Work::Value(*slot)));
                stack.push(Work::Count(extra.len()));
                for (key, slot) in extra {
                    stack.push(Work::Key(*key));
                    stack.push(Work::Value(*slot));
                }
            }
            Node::Object(members) | Node::Bare(members) => {
                // A second opcode rather than a flag byte on OBJECT, so that
                // every stream written before this existed reads unchanged
                // and the v2 golden stays what it was.
                self.out.push(match node {
                    Node::Bare(_) => OP_BARE,
                    _ => OP_OBJECT,
                });
                varint(&mut self.out, members.len() as u64);
                for (key, _) in members {
                    self.key(*key)?;
                }
                stack.extend(members.iter().map(|(_, slot)| Work::Value(*slot)));
            }
            Node::Instance { class, fields } => {
                self.out.push(OP_CLASS);
                self.class(class);
                varint(&mut self.out, fields.len() as u64);
                for (key, _) in fields {
                    self.key(*key)?;
                }
                stack.extend(fields.iter().map(|(_, slot)| Work::Value(*slot)));
            }
            Node::Map(entries) => {
                self.out.push(OP_MAP);
                varint(&mut self.out, entries.len() as u64);
                for (key, held) in entries {
                    stack.push(Work::Value(*key));
                    stack.push(Work::Value(*held));
                }
            }
            Node::Set(members) => {
                self.out.push(OP_SET);
                varint(&mut self.out, members.len() as u64);
                stack.extend(members.iter().map(|slot| Work::Value(*slot)));
            }
            Node::Date(ms) => {
                self.out.push(OP_EXT);
                self.raw(b"Date");
                varint(&mut self.out, 8);
                self.out.extend_from_slice(&date_word(*ms).to_le_bytes());
            }
            Node::Regexp { source, flags, last_index } => {
                self.out.push(OP_EXT);
                self.raw(b"RegExp");
                let payload = regexp_payload(source, flags, *last_index);
                self.raw(&payload);
            }
            Node::Error { class, message, stack: trace, cause, extra } => {
                self.out.push(OP_ERROR);
                match class {
                    ErrorClass::Builtin(name) => {
                        self.out.push(0);
                        self.string(&Str::from_str(name));
                    }
                    ErrorClass::Declared(declared) => {
                        self.out.push(1);
                        self.class(declared);
                    }
                }
                let flags = u8::from(message.is_some()) | u8::from(trace.is_some()) << 1 | u8::from(cause.is_some()) << 2;
                self.out.push(flags);
                for part in [message, trace, cause].into_iter().flatten() {
                    stack.push(Work::Value(*part));
                }
                stack.push(Work::Count(extra.len()));
                for (key, slot) in extra {
                    stack.push(Work::Key(*key));
                    stack.push(Work::Value(*slot));
                }
            }
            Node::Buffer(bytes) => {
                self.out.push(OP_ARRAYBUF);
                self.raw(bytes);
            }
            Node::NodeBuffer(bytes) => {
                self.out.push(OP_BUFFER);
                self.raw(bytes);
            }
            Node::View { kind, bytes } => {
                self.out.push(OP_VIEW);
                self.out.push(super::kinds::number_of(*kind));
                self.raw(bytes);
            }
            Node::Boxed(inner) => match *inner {
                Slot::Bits(bits) if Value(bits).as_bool().is_some() => {
                    self.out.push(OP_BOOLBOX);
                    self.out.push(u8::from(Value(bits).as_bool() == Some(true)));
                }
                Slot::Bits(bits) if Value(bits).numeric().is_some() => {
                    self.out.push(OP_NUMBOX);
                    let number = Value(bits).numeric().unwrap_or(f64::NAN);
                    self.out.extend_from_slice(&number.to_le_bytes());
                }
                other => {
                    self.out.push(OP_STRBOX);
                    stack.push(Work::Value(other));
                }
            },
            // A writer never meets these: a bigint is a slot's bits, and a
            // function returned above.
            Node::BigInt(_) | Node::Function(_) => self.out.push(OP_UNDEF),
        }
        // Pushed in stream order; the stack pops the last first, so the run
        // just pushed is reversed to come off in the order it was written.
        stack[mark..].reverse();
        Ok(())
    }

    fn primitive(&mut self, bits: u64) -> Result<(), Broken> {
        let value = Value(bits);
        match value.kind() {
            ValueKind::Float => self.number(value.numeric().unwrap_or(f64::NAN)),
            ValueKind::Int => {
                self.out.push(OP_I32);
                varint(&mut self.out, zigzag(i64::from(value.as_i32().unwrap_or(0))));
            }
            ValueKind::Bool => self.out.push(match value.as_bool() {
                Some(true) => OP_TRUE,
                _ => OP_FALSE,
            }),
            ValueKind::Singleton(number) => {
                let singletons = &self.context.singletons;
                self.out.push(match number {
                    _ if number == singletons.null => OP_NULL,
                    _ if number == singletons.hole => OP_HOLE,
                    _ => OP_UNDEF,
                });
            }
            ValueKind::Reference(_) => {
                let text = value.as_slot().and_then(|cell| self.context.text_at(cell));
                let Some(text) = text else {
                    return Err("pickle: a reference the walk did not classify".into());
                };
                self.out.push(OP_STR);
                self.string(text);
            }
            ValueKind::Client { .. } => {
                let Some(digits) = super::super::bigints::digits_of(self.context, bits) else {
                    return Err("pickle: cannot serialize a value of this kind".into());
                };
                let (negative, words) = digits.to_words();
                self.out.push(OP_BIGINT);
                self.out.push(u8::from(negative));
                varint(&mut self.out, words.len() as u64);
                for word in words {
                    self.out.extend_from_slice(&word.to_le_bytes());
                }
            }
        }
        Ok(())
    }

    /// A number: an `I32` when it is one — the common case, and one to five
    /// bytes against nine — and the double's bits otherwise. `-0` is not an
    /// integer here: its sign is the one thing it has that `0` does not.
    fn number(&mut self, number: f64) {
        let integral = number.fract() == 0.0
            && number >= f64::from(i32::MIN)
            && number <= f64::from(i32::MAX)
            && !(number == 0.0 && number.is_sign_negative());
        if integral {
            self.out.push(OP_I32);
            varint(&mut self.out, zigzag(number as i64));
            return;
        }
        self.out.push(OP_F64);
        self.out.extend_from_slice(&number.to_le_bytes());
    }

    /// A string by reference into the table: `0` then the text for the first
    /// occurrence, the entry's index plus one after that.
    fn string(&mut self, text: &Str) {
        if let Some(found) = self.strings.get(text) {
            varint(&mut self.out, found + 1);
            return;
        }
        let index = self.strings.len() as u64;
        self.strings.insert(text.clone(), index);
        self.out.push(0);
        varint(&mut self.out, wtf8_len(text) as u64);
        wtf8(&mut self.out, text);
    }

    /// A key, found by its number first.
    fn key(&mut self, key: Key) -> Result<(), Broken> {
        let named = match key {
            Key::Name(named) => named,
            Key::Index(index) => {
                self.string(&Str::from_str(&index.to_string()));
                return Ok(());
            }
        };
        if let Some(found) = self.keys.get(&named) {
            varint(&mut self.out, found + 1);
            return Ok(());
        }
        let Some(text) = self.context.interner.text(named) else {
            return Err("pickle: a property key with no text".into());
        };
        let before = self.strings.len() as u64;
        self.string(text);
        let index = self.strings.get(text).copied().unwrap_or(before);
        self.keys.insert(named, index);
        Ok(())
    }

    fn class(&mut self, class: &ClassName) {
        self.string(&class.module);
        self.string(&class.name);
        varint(&mut self.out, class.version);
    }

    fn raw(&mut self, bytes: &[u8]) {
        varint(&mut self.out, bytes.len() as u64);
        self.out.extend_from_slice(bytes);
    }
}

/// A `Date`'s time value as the v1 payload's integer — `i64::MIN` for an
/// invalid date, which has no integer of its own.
fn date_word(ms: f64) -> i64 {
    match ms.is_nan() {
        true => i64::MIN,
        false => ms as i64,
    }
}

/// v1's `RegExp` payload, kept so one reader serves both versions.
fn regexp_payload(source: &str, flags: &str, last_index: f64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(source.len() + flags.len() + 16);
    payload.extend_from_slice(&(source.len() as u32).to_le_bytes());
    payload.extend_from_slice(source.as_bytes());
    payload.extend_from_slice(&(flags.len() as u32).to_le_bytes());
    payload.extend_from_slice(flags.as_bytes());
    payload.extend_from_slice(&(last_index as i64).to_le_bytes());
    payload
}
