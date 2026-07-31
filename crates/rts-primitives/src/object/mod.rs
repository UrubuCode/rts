//! The primordial `Object`, authored entirely with the `rtse` macros.
//!
//! ## What this module is, after the drain
//!
//! `Object` reaches user code through three different mechanisms, and only ONE
//! of them belongs here:
//!
//! - **Engine-resolved protocol** — `hasOwnProperty` / `propertyIsEnumerable` /
//!   `isPrototypeOf` are answered by the engine itself
//!   (`front/run/method.rs::try_object_protocol_method`), which runs BEFORE
//!   class dispatch. Declaring them here would be unreachable duplicate surface,
//!   so they are deliberately absent.
//! - **Value-model trampolines** — `Object.keys` / `assign` / `freeze` /
//!   `create` / `fromEntries` / `groupBy` and the `Object(x)` factory are
//!   `__rtsadp_obj_*` trampolines in `rts-runtime`'s adapters. They manipulate
//!   SHAPES and PolyValue slots directly, which is the value model itself, not
//!   library surface.
//! - **The `Object.prototype` INSTANCE surface** — `toString`,
//!   `toLocaleString`, `valueOf`. Those have no native equivalent and no
//!   value-model content, so they live here, in [`value_class`], as a real
//!   `#[rtse::class("Object", value)]`.
//!
//! ## What was deleted, and why it was safe
//!
//! This module used to be a single `object.rs` carrying six hand-written
//! `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_GL_OBJECT_*` symbols — the
//! old authoring format the single-source-of-truth rule exists to drain. Every
//! one of them was **dead**: a repo-wide search for each symbol found only
//! comments, never a call site, because each had already been superseded by an
//! `__rtsadp_obj_*` trampoline on the shape-based value model.
//!
//! The `null_proto_set` that tracked `Object.create(null)` handles went with
//! them, and that deserves the explicit note: its ONLY insertion point was
//! inside the dead `__RTS_FN_GL_OBJECT_CREATE`, so `is_null_proto_handle` could
//! only ever return `false`. Its two callers were dead branches that already
//! had the live mechanism sitting next to them — the adapters record a
//! null prototype as the explicit `0` sentinel in the `__proto__` slot, which
//! `string_pool.rs` was already testing for in the very same expression.
//! Removing the always-false half changed no behaviour.

mod value_class;

pub use value_class::register;
