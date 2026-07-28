//! String-producing ABI for the `gc` namespace + generic runtime coercion
//! helpers that key only on `Entry`/`HandleTable` (`super::handles`) — the
//! primordial value-storage layer.
//!
//! Moved down from `rts-std::collector::string_pool` (2026-07-28): that file's
//! only `use` was `super::handles`, which itself is `pub use
//! rts_engine::heap::handles` — i.e. it depended on nothing `rts-std` actually
//! owns. The 39 `#[no_mangle] extern "C"` symbols it defined are called BY
//! SYMBOL from generated code (`adapter_symbols`, the lowering), so their
//! names are unchanged; only their crate moved, which turns every forward
//! `extern "C" { fn ... }` declaration that existed solely to reach them
//! (`rts-engine::gc_surface`) into a real, direct Rust call.
//!
//! A handful of the ORIGINAL functions needed `rts-shared` (Map/Set-kind
//! introspection) or `rts-primitives` (`Object.create(null)` tracking) or a
//! `rts-std` sibling (`generator::GEN_SM_DRAIN`) — those did **not** move,
//! since `rts-engine` cannot depend upward on any of the three. They remain in
//! `rts-std::collector::string_pool`: `__RTS_FN_RT_SPREAD_INTO_VEC`,
//! `__RTS_FN_RT_OBJECT_TO_STRING`, `__RTS_FN_RT_INSPECT` (+ its private
//! `inspect_handle`/`inspect_slot` helpers).

mod alloc;
mod cell;
mod coerce;
mod float_box;
mod snapshot;

pub use alloc::*;
pub use cell::*;
pub use coerce::*;
pub use float_box::*;
pub use snapshot::{EntrySnap, element_to_string, entry_kind_name, snapshot_entry, snapshot_to_bytes};

pub use crate::heap::handles::read_string_handle;
pub use crate::numfmt::format_js_number;
