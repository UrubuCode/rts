//! `AbortSignal extends EventTarget` — `aborted`/`reason`/`onabort`/
//! `throwIfAborted()` + statics `abort()`/`timeout(ms)`/`any(signals)`. Parity
//! source: `rts-shared/src/stdlib/events.ts`'s `class AbortSignal extends
//! EventTarget`.
//!
//! Composition + forwarding (DRAIN_MOTOR §11): `base: EventTarget` is an
//! embedded field (not a separate handle), so `addEventListener`/
//! `removeEventListener` FORWARD to `self.base`'s real Rust methods; firing
//! the `"abort"` event on ITSELF (from `do_abort`) calls
//! `event_target::dispatch_on(&mut self.base, self_handle, ev)` — the shared
//! dispatch body, given THIS instance's own handle so `ev.target` stamps the
//! `AbortSignal`, not a phantom base object.
//!
//! The 3 static factories (`abort`/`timeout`/`any`) all CONSTRUCT a fresh
//! `AbortSignal` instance, which the `#[rtse::statical]` macro can't express
//! (F1-F4 support scalar/`&str`/`Handle`/`Vec` returns — not `Self`; only
//! `#[rtse::ctor]` gets the `alloc_rtse` wrap). So they are hand-written
//! externs, reopening the macro-generated class (same residual pattern as
//! `event_target::target`'s `dispatchEvent`).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{
    alloc_rtse, pin_handle, unpin_handle, with_rtse, with_rtse_mut,
};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::default_abort_reason;
use crate::globals::event_target::event::Event;
use crate::globals::event_target::target::{self, EventTarget};

#[rtse::class("AbortSignal")]
#[derive(Clone, Default)]
pub struct AbortSignal {
    base: EventTarget,
    #[rtse::variable(readonly)]
    aborted: bool,
    reason: u64,
    onabort: u64,
}

#[rtse::class("AbortSignal", extends = "EventTarget")]
impl AbortSignal {
    #[rtse::ctor]
    fn new() -> Self {
        AbortSignal::default()
    }

    #[rtse::getter]
    fn reason(self: &AbortSignal) -> Handle {
        self.reason
    }

    #[rtse::getter]
    fn onabort(self: &AbortSignal) -> Handle {
        self.onabort
    }

    #[rtse::setter]
    fn set_onabort(self: &mut AbortSignal, v: Handle) {
        self.onabort = v;
    }

    #[rtse::method(name = "addEventListener", optional = 1)]
    fn add_event_listener(self: &mut AbortSignal, ty: &str, cb: Handle, opts: Handle) {
        self.base.add_event_listener(ty, cb, opts);
    }

    #[rtse::method(name = "removeEventListener")]
    fn remove_event_listener(self: &mut AbortSignal, ty: &str, cb: Handle) {
        self.base.remove_event_listener(ty, cb);
    }

    #[rtse::method(name = "throwIfAborted", throws)]
    fn throw_if_aborted(self: &AbortSignal) {
        if self.aborted {
            crate::gc_surface::__RTS_FN_RT_ERROR_SET(self.reason);
        }
    }
}

/// The shared abort transition: idempotent (a second `__doAbort` on an
/// already-aborted signal is a no-op, matching the `.ts`'s
/// `if (this.aborted) return;`), fires `"abort"` on `h` (dispatch_on over the
/// embedded base), then invokes `onabort` if set. `reason == 0` (the sentinel
/// "no explicit reason") synthesizes the default reason — now a REAL
/// `DOMException` instance (see the module doc).
pub(crate) fn do_abort(h: u64, reason: u64, default_name: &str, default_message: &str) {
    let already = with_rtse::<AbortSignal, _>(h, |s| s.map(|x| x.aborted)).unwrap_or(true);
    if already {
        return;
    }
    let reason = if reason == 0 {
        default_abort_reason(default_name, default_message)
    } else {
        reason
    };
    with_rtse_mut::<AbortSignal, _>(h, |s| {
        if let Some(s) = s {
            s.aborted = true;
            s.reason = reason;
        }
    });
    let ev = alloc_rtse("Event", Event::new_internal("abort"));
    with_rtse_mut::<AbortSignal, _>(h, |s| {
        if let Some(s) = s {
            target::dispatch_on(&mut s.base, h, ev);
        }
    });
    let onabort = with_rtse::<AbortSignal, _>(h, |s| s.map(|x| x.onabort)).unwrap_or(0);
    if onabort != 0 {
        use rts_engine::heap::poly::POLY_UNDEFINED;
        use rts_engine::heap::shapes::handle_word_auto;
        crate::gc_surface::__rtsadp_fn_invoke(
            handle_word_auto(onabort),
            handle_word_auto(ev),
            POLY_UNDEFINED,
            POLY_UNDEFINED,
            POLY_UNDEFINED,
            POLY_UNDEFINED,
        );
    }
}

/// `AbortSignal.abort(reason?)` — a signal already aborted with `reason`
/// (defaulting to `{name: "AbortError", message: "This operation was aborted"}`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORTSIGNAL_STATIC_ABORT(reason: u64) -> u64 {
    let h = alloc_rtse("AbortSignal", AbortSignal::default());
    do_abort(h, reason, "AbortError", "This operation was aborted");
    h
}

/// `AbortSignal.timeout(ms)` — a signal that aborts with a `TimeoutError`
/// after `ms` milliseconds. Runs on a detached OS thread (mirrors the prior
/// hand-written Rust implementation); the signal handle is GC-PINNED for the
/// duration (`pin_handle`/`unpin_handle`, the same mechanism `fs.watch` uses)
/// so a sweep during the sleep can't free it out from under the timer — a
/// real fix over the pre-migration Rust code, which raced the GC here.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORTSIGNAL_STATIC_TIMEOUT(ms: i64) -> u64 {
    let h = alloc_rtse("AbortSignal", AbortSignal::default());
    pin_handle(h);
    let delay = if ms > 0 { ms as u64 } else { 0 };
    std::thread::spawn(move || {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        do_abort(h, 0, "TimeoutError", "The operation timed out.");
        unpin_handle(h);
    });
    h
}

/// `AbortSignal.any(signals)` — a signal that aborts when any input signal
/// aborts (already-aborted checked synchronously first, matching the `.ts`;
/// otherwise a background poller mirrors the prior Rust implementation).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORTSIGNAL_STATIC_ANY(signals_arr: u64) -> u64 {
    use rts_engine::heap::handles::{Entry, with_entry};
    let signals: Vec<u64> = with_entry(signals_arr, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => Vec::new(),
    });
    let result = alloc_rtse("AbortSignal", AbortSignal::default());
    for &src in &signals {
        let (is_ab, reason) =
            with_rtse::<AbortSignal, _>(src, |s| s.map(|x| (x.aborted, x.reason))).unwrap_or((false, 0));
        if is_ab {
            do_abort(result, reason, "AbortError", "This operation was aborted");
            return result;
        }
    }
    pin_handle(result);
    let signals_clone = signals.clone();
    std::thread::spawn(move || {
        for _ in 0..10_000u32 {
            for &src in &signals_clone {
                let (is_ab, reason) = with_rtse::<AbortSignal, _>(src, |s| {
                    s.map(|x| (x.aborted, x.reason))
                })
                .unwrap_or((false, 0));
                if is_ab {
                    do_abort(result, reason, "AbortError", "This operation was aborted");
                    unpin_handle(result);
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_micros(500));
        }
        unpin_handle(result);
    });
    result
}

/// Reopens the macro-generated `AbortSignal` class to append the 3 hand-written
/// statics — same pattern as `event_target::target::register_event_target_class_spec`.
pub fn register_abort_signal_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("AbortSignal")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    let mut cb = e
        .class("AbortSignal")
        .extends("EventTarget")
        .doc("AbortSignal — observable abort signal (extends EventTarget).");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(static_member(
        "abort",
        Sig::with_defaults(
            vec![AbiType::Handle],
            AbiType::Handle,
            vec![rts_engine::DefaultArg::Undefined],
        ),
        "__RTS_FN_GL_ABORTSIGNAL_STATIC_ABORT",
        __RTS_FN_GL_ABORTSIGNAL_STATIC_ABORT as *const u8,
        "static abort(reason?: any): AbortSignal",
        "AbortSignal.abort(reason?) — a signal already aborted with reason.",
    ))
    .member(static_member(
        "timeout",
        Sig::new(vec![AbiType::I64], AbiType::Handle),
        "__RTS_FN_GL_ABORTSIGNAL_STATIC_TIMEOUT",
        __RTS_FN_GL_ABORTSIGNAL_STATIC_TIMEOUT as *const u8,
        "static timeout(ms: number): AbortSignal",
        "AbortSignal.timeout(ms) — aborts with TimeoutError after ms.",
    ))
    .member(static_member(
        "any",
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_ABORTSIGNAL_STATIC_ANY",
        __RTS_FN_GL_ABORTSIGNAL_STATIC_ANY as *const u8,
        "static any(signals: Iterable<AbortSignal>): AbortSignal",
        "AbortSignal.any(signals) — aborts when any input signal aborts.",
    ))
    .done();
}

#[allow(clippy::too_many_arguments)]
fn static_member(name: &str, sig: Sig, symbol: &str, fp: *const u8, ts: &str, doc: &str) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::StaticMethod,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}
