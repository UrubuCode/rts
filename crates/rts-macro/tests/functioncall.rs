//! F4 (`docs/specs/rts-macro-single-source.md`) — `#[rtse::functioncall]` is
//! the missing half of the callable-class protocol: a class declares its
//! call-WITHOUT-`new` behaviour (`Widget(x)`) separately from its constructor
//! (`new Widget(x)`). This integration test exercises the real macro
//! expansion + Registry (not a hand-rolled `Member`), asserting the two kinds
//! produce distinct members with distinct symbols, and that a Registry lookup
//! by `MemberKind::CallWithoutNew` finds the functioncall member — the same
//! query shape `crates/rts-codegen-new/src/front/run/registry.rs`'s
//! `class_functioncall` uses (that crate cannot be a dev-dependency here
//! without a cycle, so the lookup is inlined).

use rts_engine::{Engine, MemberKind};

/// A toy class with BOTH a ctor and a functioncall — `new Widget(n)` allocates
/// a wrapper; `Widget(n)` is a plain coercion, mirroring `new Boolean(x)` vs
/// `Boolean(x)`.
#[rtse::class("Widget")]
#[derive(Clone)]
struct Widget {
    n: i64,
}

#[rtse::class("Widget")]
impl Widget {
    #[rtse::ctor]
    fn new(n: i64) -> Self {
        Widget { n }
    }

    #[rtse::functioncall]
    fn call(n: i64) -> i64 {
        n * 2
    }
}

#[test]
fn functioncall_is_distinct_from_ctor() {
    let mut e = Engine::new();
    register(&mut e);
    let class = e.registry().class("Widget").expect("Widget registered");

    let ctor = class
        .members
        .iter()
        .find(|m| matches!(m.kind, MemberKind::Constructor))
        .expect("ctor member present");
    let call = class
        .members
        .iter()
        .find(|m| matches!(m.kind, MemberKind::CallWithoutNew))
        .expect("functioncall member present");

    assert_ne!(
        ctor.symbol, call.symbol,
        "ctor and functioncall must not share a symbol"
    );
    assert_eq!(ctor.fn_ptr.addr() == call.fn_ptr.addr(), false);

    // No receiver: same ABI shape as a static method (args only, no implicit
    // `this` slot), unlike `new` which returns a Handle but also takes no
    // receiver — the distinguishing fact here is the SIG length matches the
    // explicit `(n: i64)` param count, not a receiver-inflated one.
    assert_eq!(call.sig.args.len(), 1);

    // Kept out of `rts.d.ts` — reached only via the engine's protocol query,
    // never `widget.call(...)`.
    assert!(call.ts_signature.is_empty());
}

#[test]
fn functioncall_resolves_by_kind_only() {
    let mut e = Engine::new();
    register(&mut e);
    let class = e.registry().class("Widget").unwrap();

    // The query shape `registry::class_functioncall` uses: find-by-KIND, not
    // by name — there is exactly one per class.
    let hits: Vec<_> = class
        .members
        .iter()
        .filter(|m| matches!(m.kind, MemberKind::CallWithoutNew))
        .collect();
    assert_eq!(hits.len(), 1);
}
