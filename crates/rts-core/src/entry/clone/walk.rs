//! The descent: a worklist that reads inside one borrow for as long as it can.
//!
//! [`walk`] takes the borrow, classifies the root, and drains the list of
//! containers still to read. It gives the borrow back only when a container's
//! members cannot be read without running user code — an accessor, a proxy —
//! reads those outside it, and takes the borrow again to carry on. The parent
//! module says why that is the shape; this is the mechanism.

use super::classify::{Shape, refuse, shape_of};
use super::errors::Member;
use super::{DEPTH, Graph, Node, Policy, Refusal, Slot};
use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use crate::object::Key;

/// A container reserved in the arena whose members are still to be read.
struct Task {
    at: usize,
    cell: u32,
    value: u64,
    depth: usize,
    kind: Kind,
}

/// What kind of container a [`Task`] is, with what the classification already
/// learned about it.
enum Kind {
    Array,
    /// A plain object, and whether reading it runs user code.
    Object(bool),
    Instance(super::ClassName),
    Map,
    Set,
    Error,
    Boxed(u64),
}

struct Walker {
    graph: Graph,
    policy: Policy,
    tasks: Vec<Task>,
}

/// Reads one value into an arena, and answers the arena and the slot standing
/// for the value.
pub(in crate::entry) fn walk(policy: Policy, value: u64) -> Result<(Graph, Slot), Refusal> {
    let mut walker = Walker {
        graph: Graph::default(),
        policy,
        tasks: Vec::new(),
    };
    let mut root = None;
    let mut resumed: Option<(Task, Vec<(Key, u64)>)> = None;
    loop {
        let deferred = with_current(|context| -> Result<Option<Task>, Refusal> {
            if root.is_none() {
                root = Some(walker.visit(context, value, 0)?);
            }
            if let Some((task, members)) = resumed.take() {
                let mark = walker.tasks.len();
                walker.finish(context, task, members)?;
                walker.tasks[mark..].reverse();
            }
            walker.drain(context)
        })?;
        match deferred {
            None => break,
            // Read outside the borrow — this is the one place the walk runs
            // user code — and finished inside the next one.
            Some(task) => {
                let members = super::members::called(task.value)?;
                resumed = Some((task, members));
            }
        }
    }
    let root = root.unwrap_or(Slot::Bits(0));
    Ok((walker.graph, root))
}

impl Walker {
    /// Classifies one value and answers its slot: the value itself, a node
    /// complete in one step, or a container reserved now and read later.
    fn visit(&mut self, context: &mut Context, value: u64, depth: usize) -> Result<Slot, Refusal> {
        if self.policy == Policy::Clone && depth >= DEPTH {
            return Ok(Slot::Bits(undefined_of(context)));
        }
        let (cell, kind) = match shape_of(context, value, self.policy)? {
            Shape::Bits(bits) => return Ok(Slot::Bits(bits)),
            Shape::Uncloneable => return Ok(Slot::Bits(undefined_of(context))),
            // Not memoised: a function by reference resolves to the same
            // function however many times the stream names it.
            Shape::Function(name) => return Ok(Slot::At(self.graph.push(Node::Function(name)))),
            // A leaf is complete in one step, and still registered, so the same
            // `Date` twice in one structure comes back as one object twice.
            Shape::Date(cell, ms) => return Ok(self.leaf(cell, |_| Node::Date(ms), context)),
            Shape::Regexp(cell) => {
                let pickle = self.policy == Policy::Pickle;
                return Ok(self.leaf(cell, |context| regexp(context, cell, pickle), context));
            }
            Shape::Buffer(cell) => {
                return Ok(self.leaf(
                    cell,
                    |context| Node::Buffer(context.bytes_at(cell).cloned().unwrap_or_default()),
                    context,
                ));
            }
            Shape::View(cell, kind) => {
                return Ok(self.leaf(cell, |context| Node::View { kind, bytes: window(context, value) }, context));
            }
            Shape::NodeBuffer(cell) => {
                return Ok(self.leaf(cell, |context| Node::NodeBuffer(window(context, value)), context));
            }
            Shape::Array(cell) => (cell, Kind::Array),
            Shape::Object(cell, calls) => (cell, Kind::Object(calls)),
            Shape::Instance(cell, class) => (cell, Kind::Instance(class)),
            Shape::Map(cell) => (cell, Kind::Map),
            Shape::Set(cell) => (cell, Kind::Set),
            Shape::Error(cell) => (cell, Kind::Error),
            Shape::Boxed(cell, inner) => (cell, Kind::Boxed(inner)),
        };
        if let Some(at) = self.graph.found(cell) {
            return Ok(Slot::At(at));
        }
        let at = self.graph.reserve(cell);
        self.tasks.push(Task { at, cell, value, depth, kind });
        Ok(Slot::At(at))
    }

    /// A node with no children, registered under its cell.
    fn leaf(&mut self, cell: u32, node: impl FnOnce(&mut Context) -> Node, context: &mut Context) -> Slot {
        if let Some(at) = self.graph.found(cell) {
            return Slot::At(at);
        }
        let at = self.graph.reserve(cell);
        self.graph.nodes[at] = node(context);
        Slot::At(at)
    }

    /// Reads containers until none are left, or until one needs a call.
    fn drain(&mut self, context: &mut Context) -> Result<Option<Task>, Refusal> {
        while let Some(task) = self.tasks.pop() {
            // The children a container reserves are pushed in the order they
            // were met and then reversed, so the list pops them FIRST-met first
            // — which makes the walk depth-first in source order, the order a
            // getter would have run in when this was a recursion.
            let mark = self.tasks.len();
            if let Some(deferred) = self.fill(context, task)? {
                return Ok(Some(deferred));
            }
            self.tasks[mark..].reverse();
        }
        Ok(None)
    }

    /// Reads one container's members inside the borrow, or hands it back when
    /// that cannot be done without a call.
    fn fill(&mut self, context: &mut Context, task: Task) -> Result<Option<Task>, Refusal> {
        let (cell, value, depth) = (task.cell, task.value, task.depth + 1);
        let node = match &task.kind {
            Kind::Array => {
                let Some(extra) = super::members::array_extra(context, value, cell) else {
                    return Ok(Some(task));
                };
                let held = context.elements_at(cell).cloned().unwrap_or_default();
                let elements = self.each(context, &held, depth)?;
                let extra = self.members(context, extra, depth)?;
                Node::Array { elements, extra }
            }
            Kind::Object(true) => return Ok(Some(task)),
            Kind::Object(false) => {
                let Some(read) = super::members::data(context, value, cell, false) else {
                    return Ok(Some(task));
                };
                Node::Object(self.members(context, read, depth)?)
            }
            Kind::Instance(class) => {
                // The slow read cannot see a `#` field — `own_keys` hides them
                // — so an instance whose own members include an accessor has no
                // complete reading, and is refused rather than half-written.
                let Some(read) = super::members::data(context, value, cell, true) else {
                    return Err(refuse("a class instance with an own accessor property"));
                };
                let read = super::super::pickle::names::portable(context, cell, read);
                Node::Instance { class: class.clone(), fields: self.members(context, read, depth)? }
            }
            Kind::Map | Kind::Set => {
                let entries = context.table_at(cell).map(|table| table.entries()).unwrap_or_default();
                match task.kind {
                    Kind::Map => {
                        let mut pairs = Vec::with_capacity(entries.len());
                        for (key, held) in entries {
                            let key = self.visit(context, key, depth)?;
                            pairs.push((key, self.visit(context, held, depth)?));
                        }
                        Node::Map(pairs)
                    }
                    _ => {
                        let keys: Vec<u64> = entries.into_iter().map(|(key, _)| key).collect();
                        Node::Set(self.each(context, &keys, depth)?)
                    }
                }
            }
            Kind::Error => {
                let Some(read) = super::errors::read(context, value, cell, self.policy) else {
                    return Err(refuse("an Error with an own accessor property"));
                };
                let message = self.member(context, read.message, depth)?;
                let stack = self.member(context, read.stack, depth)?;
                let cause = match read.cause {
                    Some(cause) => Some(self.visit(context, cause, depth)?),
                    None => None,
                };
                let extra = self.members(context, read.extra, depth)?;
                Node::Error { class: read.class, message, stack, cause, extra }
            }
            Kind::Boxed(inner) => Node::Boxed(self.visit(context, *inner, depth)?),
        };
        self.graph.nodes[task.at] = node;
        Ok(None)
    }

    /// Finishes a container whose members were read outside the borrow.
    fn finish(&mut self, context: &mut Context, task: Task, read: Vec<(Key, u64)>) -> Result<(), Refusal> {
        let depth = task.depth + 1;
        let node = match task.kind {
            Kind::Array => {
                let held = context.elements_at(task.cell).cloned().unwrap_or_default();
                let count = held.len();
                let elements = self.each(context, &held, depth)?;
                // The slow read names every own key, indices and `length`
                // included; the elements already carry the first and the
                // array's own `length` write reproduces the second.
                let extra: Vec<(Key, u64)> = read
                    .into_iter()
                    .filter(|(key, _)| !is_element_or_length(context, *key, count))
                    .collect();
                let extra = self.members(context, extra, depth)?;
                Node::Array { elements, extra }
            }
            _ => Node::Object(self.members(context, read, depth)?),
        };
        self.graph.nodes[task.at] = node;
        Ok(())
    }

    fn each(&mut self, context: &mut Context, values: &[u64], depth: usize) -> Result<Vec<super::Slot>, Refusal> {
        let mut slots = Vec::with_capacity(values.len());
        for value in values {
            slots.push(self.visit(context, *value, depth)?);
        }
        Ok(slots)
    }

    fn members(&mut self, context: &mut Context, read: Vec<(Key, u64)>, depth: usize) -> Result<Vec<(Key, Slot)>, Refusal> {
        let mut members = Vec::with_capacity(read.len());
        for (key, value) in read {
            members.push((key, self.visit(context, value, depth)?));
        }
        Ok(members)
    }

    fn member(&mut self, context: &mut Context, member: Option<Member>, depth: usize) -> Result<Option<Slot>, Refusal> {
        Ok(match member {
            None => None,
            Some(Member::Text(text)) => Some(self.graph.text(text)),
            Some(Member::Value(value)) => Some(self.visit(context, value, depth)?),
        })
    }
}

/// A regular expression, as the two texts it is rebuilt from — and, for the
/// pickle, where its next match starts. The clone leaves `lastIndex` at `0`,
/// which is the specification's rule for a clone.
fn regexp(context: &mut Context, cell: u32, pickle: bool) -> Node {
    // The classification saw a pattern under this same borrow, so the absence
    // is unreachable rather than unhandled.
    let (source, flags) = context
        .regexp_at(cell)
        .map(|pattern| (pattern.source().to_owned(), pattern.flags().to_owned()))
        .unwrap_or_default();
    let last_index = match pickle {
        true => {
            let key = context.well_known("lastIndex");
            super::super::objects::own_property(context, cell, key)
                .and_then(|found| found.numeric())
                .unwrap_or(0.0)
        }
        false => 0.0,
    };
    Node::Regexp { source, flags, last_index }
}

/// The bytes a view covers, copied.
fn window(context: &Context, value: u64) -> Vec<u8> {
    super::super::buffers::view_of(context, value)
        .and_then(|view| super::super::buffers::window(context, &view).map(<[u8]>::to_vec))
        .unwrap_or_default()
}

/// Whether a key the slow read found is one an array's elements or its own
/// `length` already carry.
fn is_element_or_length(context: &Context, key: Key, count: usize) -> bool {
    let Key::Name(named) = key else {
        return true;
    };
    let Some(text) = context.interner.text(named).and_then(|text| text.to_rust()) else {
        return false;
    };
    text == "length" || text.parse::<usize>().is_ok_and(|index| index < count)
}
