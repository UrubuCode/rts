//! `EventTarget` — `addEventListener(type, cb, opts?)` / `removeEventListener`
//! / `dispatchEvent`. Parity source: `rts-shared/src/stdlib/events.ts`'s
//! `class EventTarget` (parallel `types`/`cbs`/`onces` arrays; duplicate
//! `(type, callback)` pairs ignored; `once` listeners removed BEFORE their
//! callback runs; `dispatchEvent` stamps `ev.target`/`ev.currentTarget` and
//! returns `!ev.defaultPrevented`).
//!
//! `dispatchEvent` needs the RECEIVER's own raw handle (to stamp
//! `ev.target`/`ev.currentTarget`), which a macro-authored method never sees
//! (the macro clones the struct OUT of the `HandleTable` before the body
//! runs — see `rts-macro`'s doc comment on instance dispatch). So
//! `dispatchEvent` is hand-written here (same residual pattern as
//! `headers::register_headers_class_spec`'s `entries`/`forEach`), reopening
//! the macro-generated class to append it. `dispatch_on` is `pub(crate)` (not
//! `#[rtse::*]`) so `AbortSignal` (a different file, `abort/signal.rs`) can
//! fire an "abort" event on ITS OWN handle through its embedded
//! `EventTarget` base — composition + forwarding, per DRAIN_MOTOR §11.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{with_rtse, with_rtse_mut};
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::handle_word_auto;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::event::Event;
use super::opt_flag;

#[rtse::class("EventTarget")]
#[derive(Clone, Default)]
pub struct EventTarget {
    types: Vec<String>,
    cbs: Vec<u64>,
    onces: Vec<bool>,
}

#[rtse::class("EventTarget")]
impl EventTarget {
    #[rtse::ctor]
    fn new() -> Self {
        EventTarget::default()
    }

    /// `addEventListener(type, cb, opts?)` — `opts` is `{once?}` (`capture`/
    /// `passive` are accepted-but-unused, matching the `.ts` baseline: it
    /// never reads them either — no regression, same gap). Duplicate
    /// `(type, callback)` pairs are ignored (spec). `pub` (not the macro's
    /// default private visibility) so `AbortSignal` (a different module) can
    /// FORWARD to its embedded `base: EventTarget` — composition, DRAIN_MOTOR §11.
    #[rtse::method(name = "addEventListener", optional = 1)]
    pub fn add_event_listener(self: &mut EventTarget, ty: &str, cb: Handle, opts: Handle) {
        for i in 0..self.types.len() {
            if self.types[i] == ty && self.cbs[i] == cb {
                return;
            }
        }
        let once = opt_flag(opts, "once");
        self.types.push(ty.to_string());
        self.cbs.push(cb);
        self.onces.push(once);
    }

    #[rtse::method(name = "removeEventListener")]
    pub fn remove_event_listener(self: &mut EventTarget, ty: &str, cb: Handle) {
        let mut kt = Vec::new();
        let mut kc = Vec::new();
        let mut ko = Vec::new();
        for i in 0..self.types.len() {
            if self.types[i] == ty && self.cbs[i] == cb {
                continue;
            }
            kt.push(self.types[i].clone());
            kc.push(self.cbs[i]);
            ko.push(self.onces[i]);
        }
        self.types = kt;
        self.cbs = kc;
        self.onces = ko;
    }
}

/// The shared dispatch body: synchronous, registration-order invocation of
/// every listener matching `ev`'s `.type`, `once` listeners removed from
/// `base` BEFORE their callback runs, `ev.target`/`ev.currentTarget` stamped
/// to `self_handle`. Returns `!ev.defaultPrevented`. Operates on an
/// already-borrowed `&mut EventTarget` so a composing class (`AbortSignal`)
/// can call it on its embedded `base` under ITS OWN handle — no chain-walk
/// dispatch needed (the `Entry::Rtse` downcast model has none; composition +
/// forwarding covers it, DRAIN_MOTOR §11).
pub(crate) fn dispatch_on(base: &mut EventTarget, self_handle: u64, ev: u64) -> bool {
    with_rtse_mut::<Event, _>(ev, |e| {
        if let Some(e) = e {
            e.set_target(self_handle);
            e.set_current_target(self_handle);
        }
    });
    let ty = with_rtse::<Event, _>(ev, |e| e.map(|x| x.kind().to_string())).unwrap_or_default();

    // Snapshot matching listeners in registration order; `once` listeners are
    // spliced out of `base` BEFORE their callback runs (spec order).
    let mut to_call: Vec<u64> = Vec::new();
    let mut i = 0usize;
    while i < base.types.len() {
        if base.types[i] == ty {
            let cb = base.cbs[i];
            if base.onces[i] {
                base.types.remove(i);
                base.cbs.remove(i);
                base.onces.remove(i);
                to_call.push(cb);
                continue; // don't advance — the vectors shifted left
            }
            to_call.push(cb);
        }
        i += 1;
    }

    let ev_word = handle_word_auto(ev);
    for cb in to_call {
        let fn_word = handle_word_auto(cb);
        crate::gc_surface::__rtsadp_fn_invoke(
            fn_word,
            ev_word,
            POLY_UNDEFINED,
            POLY_UNDEFINED,
            POLY_UNDEFINED,
            POLY_UNDEFINED,
        );
    }

    !with_rtse::<Event, _>(ev, |e| e.map(|x| x.is_default_prevented())).unwrap_or(false)
}

/// `target.dispatchEvent(ev)` — hand-written (needs `self_handle`, see the
/// module doc comment); delegates to [`dispatch_on`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENTTARGET_DISPATCH_EVENT(h: u64, ev: u64) -> i64 {
    let mut cloned: EventTarget = with_rtse::<EventTarget, _>(h, |s| s.cloned()).unwrap_or_default();
    let result = dispatch_on(&mut cloned, h, ev);
    with_rtse_mut::<EventTarget, _>(h, |slot| {
        if let Some(slot) = slot {
            *slot = cloned;
        }
    });
    result as i64
}

/// Reopens the macro-generated `EventTarget` class (ctor/addEventListener/
/// removeEventListener) to append the hand-written `dispatchEvent` — same
/// "reproduce macro members, append residuals, one `.done()`" pattern as
/// `headers::register_headers_class_spec`.
pub fn register_event_target_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("EventTarget")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    let mut cb = e
        .class("EventTarget")
        .doc("EventTarget — addEventListener/removeEventListener/dispatchEvent.");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(Member {
        name: "dispatchEvent".to_string(),
        kind: MemberKind::InstanceMethod,
        sig: Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
        symbol: "__RTS_FN_GL_EVENTTARGET_DISPATCH_EVENT".to_string(),
        fn_ptr: FnPtr(__RTS_FN_GL_EVENTTARGET_DISPATCH_EVENT as *const u8),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: "dispatchEvent(event: Event): boolean".to_string(),
        doc: "target.dispatchEvent(ev) — synchronous, spec-order dispatch; returns !ev.defaultPrevented."
            .to_string(),
        pure: false,
        emit: None,
    })
    .done();
}
