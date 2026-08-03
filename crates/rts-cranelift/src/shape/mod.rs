//! Objects whose layout is arrived at rather than declared.
//!
//! An aggregate is declared: here are the fields, here is the type. That covers
//! everything whose shape is known before it exists, and covers nothing whose
//! shape is built up one property at a time — which is most of what a dynamic
//! client allocates.
//!
//! # What a shape adds, and what it deliberately does not
//!
//! It adds one thing: **a property name to a position, in a layout that grows**.
//! Everything else it needs already exists and is reused rather than mirrored.
//!
//! A shape *is* an aggregate — it is a way of arriving at one incrementally. So
//! the object header still holds the identity of a layout, reading a field is
//! still an offset the layout decided, the type guard still compares one number,
//! and the collector still traces what the aggregate says to trace. None of
//! those learned anything.
//!
//! That is why reading a property needs no new instruction. Where the layout is
//! known, the position is known and it is an ordinary field read. Where it is
//! not, a type guard establishes the layout and then it is an ordinary field
//! read. A shape answers *which field*; it does not introduce a second way to
//! reach one.
//!
//! # The two costs, and which one is paid
//!
//! Giving every layout its own list of properties makes a lookup one indexed
//! read and makes a chain of N additions store a quadratic number of entries —
//! and building an object *is* a chain. Storing one property per layout and
//! pointing at what it extends costs one node per addition, and makes a lookup
//! proportional to how many properties there are.
//!
//! Neither is the trade it appears to be, because the walk is only paid for
//! layouts something reads, and only once: the position index is built on demand
//! and kept. A program that grows an object through fifty layouts and reads the
//! last builds one index rather than fifty. Memory follows what is looked up
//! rather than what exists.
//!
//! # Growing an object is free, and that is not an accident
//!
//! Gaining a property changes the layout, so it changes the size, so the object
//! moves. Everywhere else that is the expensive case — every reference to it has
//! to be found and rewritten. Here a reference is an index into a table, so
//! moving an object is one table entry, and nothing that refers to it notices.
//!
//! That is the second time the index-not-address decision has paid for itself,
//! after making a collector able to relocate. It was not chosen for either.

mod key;
mod tree;

pub use key::{Key, KeyRegistry};
pub use tree::{ShapeError, ShapeId, ShapeTree};
