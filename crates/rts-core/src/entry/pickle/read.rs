//! Bytes to an arena — both versions of the stream.
//!
//! # A stack, not a recursion
//!
//! Every container is a [`Frame`] on an explicit stack: it knows how many
//! children it still expects and what to read before each one (an array's
//! extra members are a key and then a value). A child that completes is handed
//! to the frame on top; a frame that has everything is closed into its node and
//! handed to the one below. So nesting costs heap and not Rust stack, and a
//! stream nested past [`MAX_DEPTH`] is refused by count rather than by a stack
//! overflow — which an `extern "C"` frame could not survive.
//!
//! # Nothing here allocates on the JavaScript heap
//!
//! The arena is plain Rust. Strings stay text until the build interns them, and
//! a class or function the stream names is looked up here — so an unknown name
//! fails before a single object exists — but nothing is constructed. That is
//! also why decoding runs no user code: it reads one table and writes an
//! arena, and the build afterwards calls no constructor, getter or setter.
//!
//! # Memo discipline
//!
//! A container is given its memo id when its opcode is read, BEFORE its
//! children, and its node is reserved at the same moment — so a child's
//! back-reference to it resolves to the reserved node, which the build makes
//! before filling any container. The writer assigns ids in the same pre-order.

use super::super::clone::{ClassName, ErrorClass, Graph, Node, Slot};
use super::super::Context;
use super::format::*;
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// Keyed members still to come: how many, what has arrived, and the key
/// waiting for its value.
struct Members {
    left: usize,
    got: Vec<(Key, Slot)>,
    key: Option<Key>,
}

/// An open container.
enum Frame {
    Array { at: usize, count: usize, elements: Vec<Slot>, extra: Option<Members> },
    /// A plain object — with no prototype when `bare` — or, when `class` is
    /// present, an instance of it.
    Object { at: usize, keys: Vec<Key>, values: Vec<Slot>, class: Option<ClassName>, bare: bool },
    Map { at: usize, count: usize, pairs: Vec<(Slot, Slot)>, key: Option<Slot> },
    Set { at: usize, count: usize, members: Vec<Slot> },
    Error { at: usize, class: ErrorClass, flags: u8, parts: Vec<Slot>, extra: Option<Members> },
    StrBox { at: usize, inner: Option<Slot> },
    /// v1: a class instance by flat name — `Map`, `Set` and the error family
    /// among them, which v1 wrote as instances of the old engine's own classes.
    Legacy { at: usize, name: Str, keys: Vec<Str>, values: Vec<Slot> },
    /// v1: `name`, `message`, and whether a cause follows.
    LegacyError { at: usize, name: Str, message: Slot, wants_cause: bool, cause: Option<Slot> },
}

/// What reading one opcode produced.
enum Opened {
    Done(Slot),
    Open(Frame),
}

pub(super) struct Reader<'a, 'c> {
    pub(super) cursor: Cursor<'a>,
    pub(super) version: u8,
    pub(super) context: &'c mut Context,
    pub(super) graph: Graph,
    memo: Vec<Slot>,
    pub(super) table: Vec<super::strings::Entry>,
    /// Each class the stream names, resolved once: by the table indices of
    /// its module and name, to the class and its private-name numbers. A
    /// stream of ten thousand instances of one class names it ten thousand
    /// times, and a registry lookup and a chain walk per instance was the
    /// cost of reading one.
    classes: std::collections::HashMap<(usize, usize), (ClassName, std::rc::Rc<Vec<Option<u32>>>)>,
}

/// The arena a stream describes, and the slot of its root.
pub(super) fn read(context: &mut Context, bytes: &[u8]) -> Result<(Graph, Slot), Broken> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(4).ok() != Some(&MAGIC[..]) {
        return Err("pickle: not an RTSP stream (bad magic)".into());
    }
    let version = cursor.byte()?;
    if version == 0 || version > VERSION {
        return Err(format!("pickle: format version {version} is not one this reads (1 to {VERSION})"));
    }
    let mut reader = Reader {
        cursor,
        version,
        context,
        graph: Graph::default(),
        memo: Vec::new(),
        table: Vec::new(),
        classes: std::collections::HashMap::new(),
    };
    let root = reader.run()?;
    if reader.cursor.left() != 0 {
        return Err("pickle: bytes after the end of the value".into());
    }
    Ok((reader.graph, root))
}

impl Reader<'_, '_> {
    fn run(&mut self) -> Result<Slot, Broken> {
        let mut stack: Vec<Frame> = Vec::new();
        let mut finished: Option<Slot> = None;
        loop {
            if let Some(slot) = finished.take() {
                match stack.last_mut() {
                    None => return Ok(slot),
                    Some(frame) => accept(frame, slot),
                }
            }
            if let Some(frame) = stack.last_mut()
                && !self.wants(frame)?
            {
                let frame = stack.pop().ok_or("pickle: unbalanced stream")?;
                finished = Some(self.close(frame)?);
                continue;
            }
            if stack.len() >= MAX_DEPTH {
                return Err(format!("pickle: nesting deeper than {MAX_DEPTH} levels"));
            }
            match self.open()? {
                Opened::Done(slot) => finished = Some(slot),
                Opened::Open(frame) => stack.push(frame),
            }
        }
    }

    /// Whether the frame on top expects another value — reading, first, any
    /// key or count that precedes it.
    fn wants(&mut self, frame: &mut Frame) -> Result<bool, Broken> {
        Ok(match frame {
            Frame::Array { count, elements, extra, .. } => {
                if elements.len() < *count {
                    return Ok(true);
                }
                if self.version < 2 {
                    return Ok(false);
                }
                if extra.is_none() {
                    *extra = Some(self.members()?);
                }
                self.member_wanted(extra.as_mut())?
            }
            Frame::Object { keys, values, .. } => values.len() < keys.len(),
            Frame::Legacy { keys, values, .. } => values.len() < keys.len(),
            Frame::Map { count, pairs, key, .. } => pairs.len() < *count || key.is_some(),
            Frame::Set { count, members, .. } => members.len() < *count,
            Frame::Error { flags, parts, extra, .. } => {
                if parts.len() < flags.count_ones() as usize {
                    return Ok(true);
                }
                if extra.is_none() {
                    *extra = Some(self.members()?);
                }
                self.member_wanted(extra.as_mut())?
            }
            Frame::StrBox { inner, .. } => inner.is_none(),
            Frame::LegacyError { wants_cause, cause, .. } => *wants_cause && cause.is_none(),
        })
    }

    /// A count of keyed members, about to be read.
    fn members(&mut self) -> Result<Members, Broken> {
        let left = self.cursor.count(2)?;
        Ok(Members { left, got: Vec::with_capacity(left), key: None })
    }

    /// Whether another keyed member follows, reading its key if one does.
    fn member_wanted(&mut self, members: Option<&mut Members>) -> Result<bool, Broken> {
        let Some(members) = members else {
            return Ok(false);
        };
        if members.got.len() >= members.left {
            return Ok(false);
        }
        if members.key.is_none() {
            members.key = Some(self.key()?);
        }
        Ok(true)
    }

    /// Reads one opcode and whatever header comes with it.
    fn open(&mut self) -> Result<Opened, Broken> {
        let singletons = self.context.singletons;
        let op = self.cursor.byte()?;
        let done = |slot| Ok(Opened::Done(slot));
        match op {
            OP_UNDEF => done(Slot::Bits(Value::from_singleton(singletons.undefined).bits())),
            OP_NULL => done(Slot::Bits(Value::from_singleton(singletons.null).bits())),
            OP_FALSE => done(Slot::Bits(Value::from_bool(false).bits())),
            OP_TRUE => done(Slot::Bits(Value::from_bool(true).bits())),
            OP_HOLE => done(Slot::Bits(Value::from_singleton(singletons.hole).bits())),
            // `from_f64` canonicalises a NaN, so no payload the stream carries
            // can land in the encoded quadrant and be read as a reference.
            OP_F64 | OP_FLOATPRIM => done(Slot::Bits(Value::from_f64(self.cursor.f64()?).bits())),
            OP_I32 => {
                let number = unzigzag(self.cursor.varint()?);
                let number = i32::try_from(number).map_err(|_| "pickle: an I32 outside 32 bits")?;
                done(Slot::Bits(Value::from_i32(number).bits()))
            }
            OP_STR => {
                let slot = match self.version {
                    1 => {
                        let text = self.raw_text()?;
                        let slot = self.graph.text(text);
                        // v1 gave a string a memo id, because the old engine's
                        // strings were heap handles like any object.
                        self.memo.push(slot);
                        slot
                    }
                    _ => self.string()?,
                };
                done(slot)
            }
            OP_REF => {
                let id = usize::try_from(self.cursor.varint()?).map_err(|_| "pickle: a bad back-reference")?;
                done(*self.memo.get(id).ok_or("pickle: a back-reference to nothing")?)
            }
            OP_ARRAY => {
                let count = self.cursor.count(1)?;
                let at = self.reserve();
                Ok(Opened::Open(Frame::Array { at, count, elements: Vec::with_capacity(count), extra: None }))
            }
            OP_OBJECT | OP_BARE => {
                let keys = self.keys()?;
                let at = self.reserve();
                let bare = op == OP_BARE;
                Ok(Opened::Open(Frame::Object { at, values: Vec::with_capacity(keys.len()), keys, class: None, bare }))
            }
            OP_CLASS if self.version == 1 => {
                let name = self.raw_text()?;
                let count = self.cursor.count(1)?;
                let mut keys = Vec::with_capacity(count);
                for _ in 0..count {
                    keys.push(self.raw_text()?);
                }
                let at = self.reserve();
                Ok(Opened::Open(Frame::Legacy { at, name, values: Vec::with_capacity(keys.len()), keys }))
            }
            OP_CLASS => {
                let (class, spaces) = self.class()?;
                let mut keys = self.keys()?;
                super::names::local(self.context, &spaces, &mut keys, false);
                let at = self.reserve();
                Ok(Opened::Open(Frame::Object {
                    at,
                    values: Vec::with_capacity(keys.len()),
                    keys,
                    class: Some(class),
                    bare: false,
                }))
            }
            OP_MAP => {
                let count = self.cursor.count(2)?;
                let at = self.reserve();
                Ok(Opened::Open(Frame::Map { at, count, pairs: Vec::with_capacity(count), key: None }))
            }
            OP_SET => {
                let count = self.cursor.count(1)?;
                let at = self.reserve();
                Ok(Opened::Open(Frame::Set { at, count, members: Vec::with_capacity(count) }))
            }
            OP_ERROR if self.version == 1 => {
                let name = self.raw_text()?;
                let message = self.raw_text()?;
                let message = self.graph.text(message);
                let at = self.reserve();
                let wants_cause = self.cursor.byte()? == 1;
                Ok(Opened::Open(Frame::LegacyError { at, name, message, wants_cause, cause: None }))
            }
            OP_ERROR => {
                let class = match self.cursor.byte()? {
                    0 => {
                        let name = self.string_text()?;
                        ErrorClass::Builtin(super::legacy::error_class(&name))
                    }
                    1 => ErrorClass::Declared(self.class()?.0),
                    _ => return Err("pickle: an error with an unknown class tag".into()),
                };
                let flags = self.cursor.byte()?;
                if flags > 0b111 {
                    return Err("pickle: an error with unknown members".into());
                }
                let at = self.reserve();
                Ok(Opened::Open(Frame::Error { at, class, flags, parts: Vec::with_capacity(3), extra: None }))
            }
            OP_STRBOX => {
                let at = self.reserve();
                Ok(Opened::Open(Frame::StrBox { at, inner: None }))
            }
            OP_BOOLBOX => {
                let flag = self.cursor.byte()? != 0;
                done(self.leaf(Node::Boxed(Slot::Bits(Value::from_bool(flag).bits()))))
            }
            OP_NUMBOX => {
                let number = self.cursor.f64()?;
                done(self.leaf(Node::Boxed(Slot::Bits(Value::from_f64(number).bits()))))
            }
            OP_BUFFER => {
                let bytes = self.cursor.block()?.to_vec();
                done(self.leaf(Node::NodeBuffer(bytes)))
            }
            OP_ARRAYBUF => {
                let bytes = self.cursor.block()?.to_vec();
                done(self.leaf(Node::Buffer(bytes)))
            }
            OP_VIEW => {
                let kind = super::kinds::kind_of(self.cursor.byte()?).ok_or("pickle: a typed array of no known kind")?;
                let bytes = self.cursor.block()?.to_vec();
                if bytes.len() % kind.size() != 0 {
                    return Err("pickle: a typed array whose bytes are not whole elements".into());
                }
                done(self.leaf(Node::View { kind, bytes }))
            }
            OP_BIGINT => {
                let negative = self.cursor.byte()? != 0;
                let count = self.cursor.count(8)?;
                let mut words = Vec::with_capacity(count);
                for _ in 0..count {
                    words.push(self.cursor.u64()?);
                }
                let at = self.graph.push(Node::BigInt(crate::bigint::BigInt::from_words(negative, &words)));
                if self.version == 1 {
                    self.memo.push(Slot::At(at));
                }
                done(Slot::At(at))
            }
            OP_EXT => {
                let tag = self.cursor.block()?;
                let payload = self.cursor.block()?;
                let node = super::legacy::extension(tag, payload)?;
                done(self.leaf(node))
            }
            OP_JSON => {
                let payload = self.cursor.block()?;
                let slot = super::legacy::json(self, payload)?;
                self.memo.push(slot);
                done(slot)
            }
            OP_FN_REF => {
                let found = match self.version {
                    1 => {
                        let name = self.raw_text()?;
                        super::names::resolve(self.context, None, &name)?
                    }
                    _ => {
                        let module = self.string_text()?;
                        let name = self.string_text()?;
                        super::names::resolve(self.context, Some(&module), &name)?
                    }
                };
                if self.version == 1 {
                    self.memo.push(Slot::Bits(found));
                }
                done(Slot::Bits(found))
            }
            other => Err(format!("pickle: unknown opcode {other}")),
        }
    }

    /// Closes a frame into its node.
    fn close(&mut self, frame: Frame) -> Result<Slot, Broken> {
        let (at, node) = match frame {
            Frame::Array { at, elements, extra, .. } => {
                let extra = extra.map(|members| members.got).unwrap_or_default();
                (at, Node::Array { elements, extra })
            }
            Frame::Object { at, keys, values, class, bare } => {
                let fields = keys.into_iter().zip(values).collect();
                match (class, bare) {
                    (Some(class), _) => (at, Node::Instance { class, fields }),
                    (None, true) => (at, Node::Bare(fields)),
                    (None, false) => (at, Node::Object(fields)),
                }
            }
            Frame::Map { at, pairs, .. } => (at, Node::Map(pairs)),
            Frame::Set { at, members, .. } => (at, Node::Set(members)),
            Frame::Error { at, class, flags, parts, extra } => {
                let mut parts = parts.into_iter();
                let mut part = |bit: u8| match flags & bit {
                    0 => None,
                    _ => parts.next(),
                };
                let (message, stack, cause) = (part(1), part(2), part(4));
                let extra = extra.map(|members| members.got).unwrap_or_default();
                (at, Node::Error { class, message, stack, cause, extra })
            }
            Frame::StrBox { at, inner } => (at, Node::Boxed(inner.ok_or("pickle: an empty String box")?)),
            Frame::Legacy { at, name, keys, values } => (at, super::legacy::class(self, name, keys, values)?),
            Frame::LegacyError { at, name, message, cause, .. } => {
                let class = ErrorClass::Builtin(super::legacy::error_class(&name));
                (at, Node::Error { class, message: Some(message), stack: None, cause, extra: Vec::new() })
            }
        };
        self.graph.nodes[at] = node;
        Ok(Slot::At(at))
    }

    /// A node reserved and memoised before its children are read.
    fn reserve(&mut self) -> usize {
        let at = self.graph.push(Node::Set(Vec::new()));
        self.memo.push(Slot::At(at));
        at
    }

    /// A node with no children, memoised.
    fn leaf(&mut self, node: Node) -> Slot {
        let slot = Slot::At(self.graph.push(node));
        self.memo.push(slot);
        slot
    }

    /// A class a v2 stream names, resolved in this program.
    fn class(&mut self) -> Result<(ClassName, std::rc::Rc<Vec<Option<u32>>>), Broken> {
        let module = self.entry()?;
        let name = self.entry()?;
        let version = self.cursor.varint()?;
        if let Some((class, spaces)) = self.classes.get(&(module, name)) {
            return Ok((ClassName { version, ..class.clone() }, spaces.clone()));
        }
        let (module_text, name_text) = (self.entry_text(module), self.entry_text(name));
        let found = super::names::resolve(self.context, Some(&module_text), &name_text)?;
        let prototype = super::legacy::prototype_of(self.context, found, &name_text)?;
        let spaces = Value(prototype)
            .as_slot()
            .map(|prototype| super::names::spaces(self.context, prototype))
            .unwrap_or_default();
        let class = ClassName {
            module: std::rc::Rc::new(module_text),
            name: std::rc::Rc::new(name_text),
            prototype,
            version,
        };
        let spaces = std::rc::Rc::new(spaces);
        self.classes.insert((module, name), (class.clone(), spaces.clone()));
        Ok((class, spaces))
    }

    /// A block of `count` keys, each a string of the table.
    fn keys(&mut self) -> Result<Vec<Key>, Broken> {
        let count = self.cursor.count(1)?;
        let mut keys = Vec::with_capacity(count);
        for _ in 0..count {
            keys.push(match self.version {
                1 => {
                    let text = self.raw_text()?;
                    Key::Name(self.context.interner.intern(&text, &mut self.context.keys))
                }
                _ => self.key()?,
            });
        }
        Ok(keys)
    }
}

/// Hands a completed value to the frame waiting for it.
fn accept(frame: &mut Frame, slot: Slot) {
    let keyed = |members: &mut Option<Members>| {
        if let Some(members) = members
            && let Some(key) = members.key.take()
        {
            members.got.push((key, slot));
        }
    };
    match frame {
        Frame::Array { count, elements, extra, .. } => match elements.len() < *count {
            true => elements.push(slot),
            false => keyed(extra),
        },
        Frame::Object { values, .. } | Frame::Legacy { values, .. } => values.push(slot),
        Frame::Map { pairs, key, .. } => match key.take() {
            None => *key = Some(slot),
            Some(held) => pairs.push((held, slot)),
        },
        Frame::Set { members, .. } => members.push(slot),
        Frame::Error { flags, parts, extra, .. } => match parts.len() < flags.count_ones() as usize {
            true => parts.push(slot),
            false => keyed(extra),
        },
        Frame::StrBox { inner, .. } => *inner = Some(slot),
        Frame::LegacyError { cause, .. } => *cause = Some(slot),
    }
}
