//! `Event` + `EventTarget` global classes (#63) — DRAIN_MOTOR §11 (owner
//! 2026-07-24 correction): reimplemented as `#[rtse::class]` at FULL PARITY
//! with the live `.ts` polyfill (`rts-shared/src/stdlib/events.ts`), which is
//! the authoritative semantics source (options bag `{once}`, spec dispatch
//! order, `once` removed BEFORE its callback runs, `!defaultPrevented`
//! return). The earlier reimplementation attempt shipped a PLAINER baseline
//! (no options, no `once`) and was reverted as a regression — this one closes
//! that gap before trimming the `.ts`.
//!
//! One `#[rtse::class]` per file (two impls in one file collide on the
//! generated `pub fn register`): `event.rs` (`Event`), `target.rs`
//! (`EventTarget`). `AbortSignal`/`AbortController` (which `extends
//! "EventTarget"`) live in the sibling `abort/` module.

pub mod event;
pub mod target;

pub use event::register as register_event_class_spec;
pub use target::register_event_target_class_spec;

/// Read a boolean option `key` from an options-bag handle (a keyed object;
/// `0` = absent bag). Resolves through the engine's own dynamic property read
/// + ToBoolean (link-resolved externs — no crate cycle), same pattern as
/// `text_encoding::decoder::opt_flag`.
pub(crate) fn opt_flag(opts_h: u64, key: &str) -> bool {
    unsafe extern "C" {
        fn __rtsadp_obj_get(obj_word: u64, key_word: u64) -> u64;
        fn __rtsadp_to_boolean(word: u64) -> u64;
    }
    use rts_engine::heap::handles::{Entry, __RTS_FN_NS_GC_POLY_FROM_HANDLE, alloc_entry};
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_OBJECT, POLY_TAG_SHIFT, POLY_TAG_STR};
    if opts_h == 0 {
        return false;
    }
    let obj_word =
        POLY_BOX_BASE | (POLY_TAG_OBJECT << POLY_TAG_SHIFT) | __RTS_FN_NS_GC_POLY_FROM_HANDLE(opts_h);
    let key_h = alloc_entry(Entry::String(key.as_bytes().to_vec()));
    let key_word =
        POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | __RTS_FN_NS_GC_POLY_FROM_HANDLE(key_h);
    unsafe { __rtsadp_to_boolean(__rtsadp_obj_get(obj_word, key_word)) != 0 }
}
