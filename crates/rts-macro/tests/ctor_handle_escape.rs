//! Ctor escape hatch (`-> Handle`) — a `#[rtse::ctor]` whose Rust return type
//! is `Handle` opts OUT of the default `alloc_rtse` box (see
//! `crates/rts-macro/src/class/member/returns.rs`). This is the shape
//! `Proxy` uses to allocate its own dedicated `Entry::Proxy` instead of a
//! boxed `Entry::Rtse<Proxy>` struct. Proven two ways below: the returned
//! handle resolves through `with_entry` to the EXACT `Entry` the ctor body
//! built, verbatim; and `with_rtse::<HandleCtor, _>` on that same handle finds
//! nothing, confirming it was never boxed as `Entry::Rtse<HandleCtor>`.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_rtse};

#[rtse::class("HandleCtor")]
pub struct HandleCtor;

#[rtse::class("HandleCtor")]
impl HandleCtor {
    /// Allocates its OWN `Entry::String` directly, bypassing `alloc_rtse`
    /// entirely — the same escape hatch `Proxy` uses for `Entry::Proxy`.
    #[rtse::ctor]
    fn new(tag: i64) -> Handle {
        alloc_entry(Entry::String(format!("tag:{tag}").into_bytes()))
    }
}

#[test]
fn ctor_handle_escape_returns_raw_handle_not_boxed_self() {
    let h = __rtsm_global_handlector_new(7);
    assert_ne!(h, 0, "ctor must return a live handle");

    // The returned word IS the dedicated `Entry::String` the ctor body built —
    // proving `-> Handle` passed the raw allocation through untouched.
    let is_expected_string =
        with_entry(h, |e| matches!(e, Some(Entry::String(b)) if b == b"tag:7"));
    assert!(
        is_expected_string,
        "expected the raw Entry::String the ctor allocated, not an alloc_rtse box"
    );

    // It is NOT an `Entry::Rtse<HandleCtor>` box — `with_rtse` finds nothing.
    let boxed: Option<HandleCtor> = with_rtse::<HandleCtor, _>(h, |o| o.map(|_| HandleCtor));
    assert!(
        boxed.is_none(),
        "a ctor returning `Handle` must not go through alloc_rtse"
    );
}
