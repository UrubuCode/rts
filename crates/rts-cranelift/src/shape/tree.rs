//! How an object's layout is arrived at, one property at a time.

// Keyed by numbers this crate mints — a shape, a key, a representation — so
// there is no untrusted input to be resistant to and SipHash is buying nothing.
// The MAP a JavaScript `Map` is built on is a different question with a
// different answer, and it is not this one.
use rustc_hash::FxHashMap as HashMap;

use super::Key;
use crate::repr::Repr;
use crate::types::{TypeId, TypeRegistry};

/// A layout, reached by adding properties in a particular order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ShapeId(pub(crate) u32);

impl ShapeId {
    /// The tree index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Why a transition was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeError {
    /// The property is already there, held differently than the one being added.
    ///
    /// Not a transition: the key list would be unchanged while a slot's
    /// representation changed underneath compiled code that had already been
    /// told what was in it. Reaching a layout where that key is held the other
    /// way is [`ShapeTree::redefine`], which produces a different shape — and a
    /// different shape is one guarded code will not mistake for this one.
    AlreadyHeldDifferently {
        /// The property.
        key: Key,
        /// How the existing layout holds it.
        existing: Repr,
        /// How the transition asked for it.
        asked: Repr,
    },
}

/// One step: a layout, plus the property that distinguishes it from its parent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Node {
    parent: Option<ShapeId>,
    key: Key,
    repr: Repr,
    /// How many properties this layout has.
    ///
    /// Kept rather than counted, because it is asked for on every lookup and
    /// walking to the root to count is the cost the memo below exists to avoid.
    width: u32,
}

/// Every layout one compilation knows about.
///
/// # Why a chain and not a list per layout
///
/// The obvious representation gives each layout its own list of properties, so a
/// lookup is one indexed read. It also means a chain of N additions stores
/// 1 + 2 + ... + N entries, which is quadratic in the length of the chain — and a
/// chain is what building an object *is*.
///
/// So a layout stores one property and points at what it extends. The tree costs
/// one node per addition, and two objects built the same way arrive at the same
/// node, which is what makes comparing a single number a complete answer to
/// "are these laid out alike".
///
/// # Why lookup is still fast
///
/// A chain answers "where is this property" by walking, which is proportional to
/// how many properties there are. That would be the trade — except that the walk
/// is only paid for layouts a program actually reads, and it is paid once.
///
/// The index below is built on demand and kept. A program that grows an object
/// through fifty layouts and then reads the last one builds one index, not fifty:
/// the memory is proportional to what is looked up rather than to what exists,
/// which is the distinction the quadratic version does not make.
#[derive(Default)]
pub struct ShapeTree {
    nodes: Vec<Node>,
    /// Where each (parent, key, representation) step already leads.
    ///
    /// This is what makes two objects built the same way share a layout, which
    /// is the whole reason a shape identifies anything.
    transitions: HashMap<(Option<ShapeId>, Key, Repr), ShapeId>,
    /// Property positions, for the layouts something has looked one up in.
    ///
    /// # Why a `Vec` and not a map keyed by the shape
    ///
    /// Because a `ShapeId` is **dense** — it is `ShapeId(self.nodes.len())`, so
    /// the numbers are 0, 1, 2, … with no gaps — and hashing a dense number to
    /// find a position is paying for a search that has an answer by arithmetic.
    ///
    /// It was `HashMap<ShapeId, HashMap<Key, u32>>`, and [`Self::slot_of`] paid
    /// **three hash probes** for one property read: `contains_key(&shape)`, then
    /// `self.indexes[&shape]` hashing the same shape again, then `get(&key)` on
    /// the inner map. Two of the three asked the same question about a number
    /// that indexes an array.
    ///
    /// `None` at a position means the layout exists and nobody has looked a
    /// property up in it yet; the vector is grown to fit rather than sized in
    /// advance, because a program that never reads a property of a shape should
    /// not pay for its index.
    indexes: Vec<Option<HashMap<Key, u32>>>,
    /// The empty layout's index, which is always empty and has no position.
    ///
    /// Beside the vector rather than in it because [`ShapeTree::root`] is
    /// `ShapeId(u32::MAX)` — a sentinel, since the empty layout is not a node —
    /// and a vector long enough to hold that subscript is four billion entries.
    root_index: Option<HashMap<Key, u32>>,
    /// The registered aggregate each layout became, for the ones that needed one.
    types: HashMap<ShapeId, TypeId>,
}

impl ShapeTree {
    /// A tree holding only the empty layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// The layout with no properties.
    ///
    /// Every object starts here, which is why it is a fact about the tree rather
    /// than something a client creates and could create twice.
    pub fn root(&self) -> ShapeId {
        ShapeId(u32::MAX)
    }

    /// The layout reached by adding a property to another.
    ///
    /// Adding one that is already there, held the same way, is not a transition:
    /// assigning to a property an object already has does not change its shape.
    /// Adding one held differently is refused — see [`ShapeError`].
    ///
    /// Properties are appended, never inserted, so a position handed out once
    /// stays correct however much the layout grows afterwards. That is what lets
    /// compiled code keep a position it was told about.
    pub fn transition(
        &mut self,
        from: ShapeId,
        key: Key,
        repr: Repr,
    ) -> Result<ShapeId, ShapeError> {
        if let Some(existing) = self.repr_of(from, key) {
            return if existing == repr {
                Ok(from)
            } else {
                Err(ShapeError::AlreadyHeldDifferently {
                    key,
                    existing,
                    asked: repr,
                })
            };
        }

        let parent = self.parent_key(from);
        if let Some(&existing) = self.transitions.get(&(parent, key, repr)) {
            return Ok(existing);
        }

        let width = self.width(from) + 1;
        let id = ShapeId(self.nodes.len() as u32);
        self.nodes.push(Node {
            parent: self.parent_of_new(from),
            key,
            repr,
            width,
        });
        self.transitions.insert((parent, key, repr), id);
        Ok(id)
    }

    /// The layout that is this one without a property.
    ///
    /// Rebuilt from the empty layout rather than unlinked, because the tree only
    /// grows: a node is shared by everything that extends it, and unlinking one
    /// would change a layout other objects are already using.
    ///
    /// The result is a different layout with a different identity, so code that
    /// was told where a property sits in the old one will not touch an object in
    /// the new one — the guard it was compiled with sees a different number.
    /// Removing is therefore correct rather than merely allowed, and expensive,
    /// which matches how often programs do it.
    pub fn remove(&mut self, from: ShapeId, key: Key) -> ShapeId {
        let kept: Vec<_> = self
            .properties(from)
            .into_iter()
            .filter(|(existing, _)| *existing != key)
            .collect();

        let mut shape = self.root();
        for (existing, repr) in kept {
            shape = self
                .transition(shape, existing, repr)
                .expect("rebuilding a layout adds each property once");
        }
        shape
    }

    /// The same layout with one property held differently.
    ///
    /// Also a rebuild, and for the same reason: what changes is not where a
    /// property sits but what is in it, and compiled code was told both.
    pub fn redefine(&mut self, from: ShapeId, key: Key, repr: Repr) -> ShapeId {
        let rebuilt: Vec<_> = self
            .properties(from)
            .into_iter()
            .map(|(existing, held)| {
                if existing == key {
                    (existing, repr)
                } else {
                    (existing, held)
                }
            })
            .collect();

        let mut shape = self.root();
        for (existing, held) in rebuilt {
            shape = self
                .transition(shape, existing, held)
                .expect("rebuilding a layout adds each property once");
        }
        shape
    }

    /// Where a property sits, if the layout has it.
    ///
    /// The position is the field index of the registered aggregate this layout
    /// becomes, so it is the same number [`crate::ir::FuncBuilder::field_load`]
    /// takes. A shape does not introduce a second way to reach a field; it
    /// introduces a way to find out which field.
    pub fn slot_of(&mut self, shape: ShapeId, key: Key) -> Option<u32> {
        self.index_of(shape).get(&key).copied()
    }

    /// How a property is held, if the layout has it.
    pub fn repr_of(&self, shape: ShapeId, key: Key) -> Option<Repr> {
        let mut current = shape;
        while let Some(node) = self.node(current) {
            if node.key == key {
                return Some(node.repr);
            }
            current = node.parent?;
        }
        None
    }

    /// Every property, in the order they were added.
    pub fn properties(&self, shape: ShapeId) -> Vec<(Key, Repr)> {
        let mut backwards = Vec::new();
        let mut current = shape;
        while let Some(node) = self.node(current) {
            backwards.push((node.key, node.repr));
            match node.parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        backwards.reverse();
        backwards
    }

    /// How many properties a layout has.
    pub fn width(&self, shape: ShapeId) -> u32 {
        self.node(shape).map(|node| node.width).unwrap_or(0)
    }

    /// The registered aggregate this layout is.
    ///
    /// Declared once and kept. A shape is a way of arriving at a layout; what it
    /// arrives at is an ordinary aggregate, which is what lets everything that
    /// already understands one — the object header, reading a field, the
    /// collector deciding what to trace — work without knowing shapes exist.
    pub fn layout(&mut self, shape: ShapeId, types: &mut TypeRegistry) -> TypeId {
        if let Some(&existing) = self.types.get(&shape) {
            return existing;
        }

        let fields: Vec<_> = self
            .properties(shape)
            .into_iter()
            .map(|(_, repr)| repr)
            .collect();
        let id = types.declare(&fields);
        self.types.insert(shape, id);
        id
    }

    /// How many layouts exist.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether only the empty layout does.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Property positions for a layout, built on first use and kept.
    fn index_of(&mut self, shape: ShapeId) -> &HashMap<Key, u32> {
        // Grown to fit rather than probed for. A `ShapeId` is dense, so "does
        // this layout have an index yet" is a bounds check and an `is_none`,
        // where it used to be a hash of the shape — twice, once to ask and once
        // to take. See the field for what that cost.
        //
        // THE ROOT IS THE EXCEPTION and it has to be named. `ShapeTree::root` is
        // `ShapeId(u32::MAX)`, a sentinel rather than a position — the empty
        // layout is not a node, which is what `parent_key` records. Indexing by
        // it asks for a vector of four billion entries: the first build of this
        // died with "memory allocation of 137438953472 bytes failed", which is
        // the sentinel being read as a subscript.
        //
        // It is answered from a field of its own rather than by reserving a
        // position, because the alternative is a vector whose length is
        // `u32::MAX` and whose every entry but two is `None`.
        if shape == self.root() {
            if self.root_index.is_none() {
                self.root_index = Some(HashMap::default());
            }
            return self
                .root_index
                .as_ref()
                .expect("the empty layout was just filled");
        }
        let at = shape.index();
        if self.indexes.len() <= at {
            self.indexes.resize_with(at + 1, || None);
        }
        if self.indexes[at].is_none() {
            let index = self
                .properties(shape)
                .into_iter()
                .enumerate()
                .map(|(position, (key, _))| (key, position as u32))
                .collect();
            self.indexes[at] = Some(index);
        }
        self.indexes[at]
            .as_ref()
            .expect("the position was just filled")
    }

    fn node(&self, shape: ShapeId) -> Option<&Node> {
        self.nodes.get(shape.index())
    }

    /// What a step out of `from` is keyed by.
    ///
    /// The empty layout is not a node, so a step out of it is keyed by nothing —
    /// which is what makes every object that gains its first property the same
    /// way arrive at the same layout.
    fn parent_key(&self, from: ShapeId) -> Option<ShapeId> {
        self.node(from).map(|_| from)
    }

    fn parent_of_new(&self, from: ShapeId) -> Option<ShapeId> {
        self.parent_key(from)
    }
}
