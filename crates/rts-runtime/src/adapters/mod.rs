//! The runtime-side value model of the new engine: the NaN-boxed
//! [`value::PolyValue`] and every `__rtsadp_*` runtime trampoline (`value::*`),
//! plus [`dispatch`] — the small static method-metadata table some of those
//! trampolines consult at runtime (e.g. the `in` operator's array-proto probe).
//!
//! Formerly the standalone `rts-adapters` crate, folded into `rts-runtime` so
//! ONE crate carries both the trampolines and the runtime they call into, and
//! that crate is the AOT staticlib archive.
//!
//! NB the two-step build did NOT disappear, it MOVED: Cargo emits a staticlib
//! only for a package built as a DIRECT TARGET, and being a direct dependency
//! of the `rts` bin is not the same thing. `cargo build -p rts-runtime` is now
//! the pre-step that `cargo build -p rts-adapters` used to be — verified by
//! measurement, since a plain `cargo build` left a stale archive with no
//! `__rtsadp_*` symbols and AOT failed to link. See
//! `docs/specs/rts-codegen-new-design.md` for why this module exists at all:
//! the compile-time-only slices of the old value model (the `repr` lattice,
//! hidden-class `shape`s, `state::reset_codegen_state`) live in
//! `rts-codegen-new` instead — they are lowering-time concerns, not AOT-linked
//! runtime surface.
pub mod dispatch;

pub mod value;
