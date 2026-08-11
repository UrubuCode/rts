//! N-API — the symbols a `.node` addon resolves out of this process.
//!
//! Read `README.md` before changing anything here: the ABI is not ours to
//! design, a `napi_value` is a slot rather than a value, and a failure is a
//! status rather than a panic. `PLAN.md` has the phases and what each one is
//! waiting on.
//!
//! # What works today
//!
//! P1: handle scopes, and the value surface — numbers, booleans, `undefined`,
//! `null`, UTF-8 strings both ways, `napi_typeof`.
//!
//! P2: objects and their properties by key, by name and by index; property
//! names; arrays and their elements.
//!
//! P3: calling, both directions — a JS callable whose body is the addon's, the
//! addon calling a JS function, and `napi_get_cb_info`.
//!
//! P4: references — a value an addon keeps after the call that produced it,
//! strongly while its refcount is above zero and weakly at zero.
//!
//! P5: an addon's own pointer behind a JavaScript object, and externals.
//!
//! P6: finalizers — run by the collector, at the next drain rather than during
//! the sweep.
//!
//! P7a: async work — `execute` on a worker thread, `complete` back on the
//! JavaScript thread.
//!
//! P7b: threadsafe functions — any thread asks, the JavaScript thread calls.
//!
//! P7c: errors — throwing a value, building one, and asking what is pending.
//!
//! P7d: classes — `napi_define_class`, property descriptors, `new`.
//!
//! P7e: buffers — bytes an addon writes in place, through a real pointer.
//!
//! P8a: registration — an addon saying what it exports.
//!
//! P8b: the export table — every symbol handed to the linker.
//!
//! P7f: the rest of the value surface — integer widths, the four coercions,
//! `===`, and `globalThis`.
//!
//! P7f: the rest of the value surface — integer widths, the four coercions,
//! `===`, and `globalThis`.
//!
//! P8c: the loader — mapping a `.node` and finding its entry point.
//!
//! Everything else in the ABI is absent rather than stubbed; an absent symbol
//! fails to link loudly, which is the answer an addon can act on.
//!
//! # Where the tests are
//!
//! `tests/addon.rs`, not beside the code. They state the invariants in the
//! PUBLIC vocabulary, because that is the only vocabulary a `.node` has — the
//! same split, for the same reason, `rts-cranelift`'s README describes. What
//! stays here as a unit test is what has no public surface to state it in: the
//! ABI's own numbering, in `abi.rs`.
//!
//! # Why the names look like that
//!
//! `napi_status`, `napi_create_double`, `NAPI_AUTO_LENGTH` — snake case, no
//! Rust convention, no attribute deriving them. They are a foreign C interface
//! whose spelling IS the contract, which `CLAUDE.md` names as the one permanent
//! exception to "never hand-write a symbol name".

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![deny(missing_docs)]
#![deny(dead_code)]

pub mod abi;
pub mod async_work;
pub mod bigints;
pub mod buffers;
pub mod class;
pub mod cleanup;
pub mod convert;
pub mod dates;
pub mod env;
pub mod errors;
pub mod exported;
pub mod finalizers;
pub mod functions;
pub mod handles;
pub mod instance;
pub mod integrity;
pub mod loader;
pub mod module;
pub mod objects;
pub mod references;
pub mod rest;
pub mod scopes;
pub mod tags;
pub mod threadsafe;
pub mod values;
pub mod wrap;

pub use abi::{napi_env, napi_status, napi_value, napi_valuetype};
pub use env::Env;

