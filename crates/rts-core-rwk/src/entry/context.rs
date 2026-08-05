//! What the context answers about itself.
//!
//! # Why this is beside the struct rather than in it
//!
//! `mod.rs` declares what a running program's operations need, and the fields
//! carry a page of reasoning each — which is the file's job. These are the
//! questions asked *of* those fields, and keeping them here is what let the
//! declaration stay readable when the eighth side table arrived.
//!
//! A child module rather than a sibling, and that is load-bearing: a private
//! field is visible to the module that declares it and to its descendants, so
//! `entry::context` reaches them and a crate-level `context` module would not.

use super::Context;
use crate::heap::Slot;
use crate::text::Str;
use crate::value::Value;

impl Context {
    /// The layout a shape arrives at, remembering the way back.
    ///
    /// `ShapeTree::layout` is what turns a shape into an aggregate. The reverse
    /// is recorded here because a cell's header holds the TYPE and a property
    /// lookup needs the SHAPE — and searching every layout per access is the
    /// cost this design exists to remove.
    pub fn layout_of(
        &mut self,
        shape: rts_cranelift::shape::ShapeId,
    ) -> rts_cranelift::types::TypeId {
        let ty = self.shapes.layout(shape, &mut self.types);
        if self.shape_of_type.len() <= ty.index() {
            self.shape_of_type.resize(ty.index() + 1, shape);
        }
        self.shape_of_type[ty.index()] = shape;
        ty
    }

    /// Which shape a cell's type came from, if it is an object's.
    ///
    /// `None` for a string's layout and for a callable's, which is what makes a
    /// reference's kind readable from the object rather than from the encoding
    /// — the machine's own answer to a tag space that has no room for one.
    ///
    /// # Why both reserved layouts have to be named here
    ///
    /// Because `shape_of_type` is grown with `resize(index + 1, shape)`, which
    /// fills every new position with the shape being recorded. So the moment an
    /// object's layout is numbered above a reserved one, the reserved one's
    /// position holds a real shape that was never its own — and a callable
    /// would answer property reads by interpreting its code address as a field.
    ///
    /// Excluding by index rather than fixing the fill: the reserved layouts are
    /// the two positions that legitimately have no shape, and saying so is the
    /// fact, where a sentinel fill would be a way of encoding it.
    pub fn shape_of(&self, ty: u32) -> Option<rts_cranelift::shape::ShapeId> {
        let ty = ty as usize;
        if ty == self.text_type.index() {
            return None;
        }
        self.shape_of_type.get(ty).copied()
    }

    /// What a cell calls, if it is callable.
    ///
    /// A method rather than a public field so nothing outside this module can
    /// claim a cell is callable, which is what makes the code address
    /// unreachable from anything a program can write.
    pub(super) fn callable_at(&self, cell: u32) -> Option<(u64, u64)> {
        self.callables.copied(cell)
    }

    /// What a cell inherits from, if anything.
    pub(super) fn prototype_at(&self, cell: u32) -> Option<u64> {
        self.prototypes.copied(cell)
    }

    /// Sets what a cell inherits from.
    pub(super) fn set_prototype(&mut self, cell: u32, prototype: u64) {
        self.prototypes.set(cell, prototype);
    }

    /// Whether a callable asks its parent for the object it builds.
    pub(super) fn is_derived(&self, cell: u32) -> bool {
        self.derived.copied(cell).unwrap_or(false)
    }

    /// Records that it does.
    pub(super) fn mark_derived(&mut self, cell: u32) {
        self.derived.set(cell, true);
    }

    /// Records that a cell calls this code with this environment.
    pub(super) fn mark_callable(&mut self, cell: u32, code: u64, environment: u64) {
        self.callables.set(cell, (code, environment));
    }

    /// The text a reference names, if it names one.
    ///
    /// A reference is a REGION index now, uniformly — for a string as much as
    /// for an object. Its cell holds the string's type in the header and where
    /// the text is in its first slot; the text itself is in the slab, because a
    /// string is any length and a cell is 64 bytes.
    ///
    /// That indirection is not a compromise. String identity and string data are
    /// separate things in every engine that moves either one, and putting the
    /// identity in the region is what lets one reference space serve both kinds.
    pub fn text_at(&self, reference: u32) -> Option<&Str> {
        if self.region.type_of(reference)? as usize != self.text_type.index() {
            return None;
        }
        let slot = self.region.field(reference, 0)? as u32;
        self.cells.at(Slot(slot)).ok()
    }

    /// Put a string on the heap and return the value naming it.
    pub fn intern_value(&mut self, text: Str) -> Value {
        let slot = self.cells.insert(text).slot();
        let size = crate::heap::STRIDE;
        let ty = self.text_type.index() as u32;
        let cell = self.region.alloc(size, ty).expect("the region has room");
        self.region
            .set_field(cell, 0, u64::from(slot.0))
            .expect("a string cell has a first slot");
        Value::from_slot(cell)
    }

    /// Whether two slots hold equal text.
    ///
    /// What `===` needs and cannot answer alone: two strings are equal when
    /// their text is, however they were allocated, while two objects are equal
    /// only when they are the same object.
    pub fn same_text(&self, left: u32, right: u32) -> bool {
        match (self.text_at(left), self.text_at(right)) {
            (Some(a), Some(b)) => a.same_units(b),
            _ => false,
        }
    }
}

impl Context {
    /// The key a name the runtime itself knows has.
    ///
    /// `length` and `prototype` are properties this crate reads by name rather
    /// than by a number a compilation resolved, because it is the runtime that
    /// wants them — an array answers `length` whether or not the program ever
    /// wrote it, and `new` reads `prototype` on a function the program may
    /// never have touched.
    ///
    /// Interned rather than held as a constant: the number is whatever the
    /// registry issued, and that registry was seeded from what the compilation
    /// resolved. A program that mentions the name already put it there and this
    /// finds the same number; one that never does mints one nothing else uses,
    /// which costs a key and changes no answer.
    ///
    /// One function because it was two, and "intern a name the runtime knows"
    /// is one rule — the second copy is the one that would have interned
    /// against a different registry the day there were two.
    pub(super) fn well_known(&mut self, name: &str) -> crate::object::Key {
        let text = Str::from_str(name);
        crate::object::Key::Name(self.interner.intern(&text, &mut self.keys))
    }
}
