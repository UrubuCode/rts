//! `EventEmitter` global class — hybrid sync/async listener dispatch.
//!
//! Authored with the `#[rtse::class]` macro (like `RtsePoint`): a NORMAL Rust
//! struct (`EmitterData`) + `impl`; the macro generates the extern-C ABI glue and
//! `register(e)`. Storage is the generic `Entry::Rtse` (the macro allocs the ctor
//! result via `alloc_rtse`). The hand-written `__RTS_FN_GL_EE_*` externs + the
//! hand-built `Member{}` rows are gone.
//!
//! Listeners are function VALUES: each `on`/`once`/… stores the raw NaN-boxed
//! PolyValue word (`Poly` param, ABI-unchanged). `emit` snapshots + drops `once`s
//! UNDER the entry lock, then invokes each listener OUTSIDE the lock (so a
//! reentrant `on`/`emit` from inside a listener sees the real, up-to-date data —
//! it does NOT ride the macro's clone/write-back path, which would clobber such a
//! reentrant mutation). That is why `emit`/`emitHandle` take `&self` + a
//! `SelfHandle` and mutate the real handle directly, rather than `&mut self`.

use std::collections::HashMap;

use rts_engine::abi::ty::{Handle, Poly, SelfHandle, U64};
use rts_engine::heap::handles::{free_handle, with_rtse_mut};
use rts_engine::heap::poly::POLY_UNDEFINED;

#[derive(Clone)]
struct Listener {
    fn_ptr: u64,
    once: bool,
}

/// The backing store of an `EventEmitter` instance (a classed `Entry::Rtse`).
#[rtse::class("EventEmitter")]
#[derive(Clone)]
pub struct EmitterData {
    listeners: HashMap<String, Vec<Listener>>,
    async_mode: bool,
    max_listeners: i64,
}

#[rtse::class("EventEmitter")]
impl EmitterData {
    /// new EventEmitter()
    #[rtse::ctor]
    fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            async_mode: false,
            max_listeners: 10,
        }
    }

    /// new EventEmitter(async) — fire-and-forget dispatch when `async` is true.
    #[rtse::ctor]
    fn new_async(is_async: bool) -> Self {
        Self {
            listeners: HashMap::new(),
            async_mode: is_async,
            max_listeners: 10,
        }
    }

    /// emitter.free() — libera o handle.
    #[rtse::method(name = "free")]
    fn free(self: &EmitterData, me: SelfHandle) -> i64 {
        if free_handle(me) {
            1
        } else {
            0
        }
    }

    /// emitter.on(event, listener) — append a listener; returns `this`.
    #[rtse::method(name = "on", returns = "EventEmitter")]
    fn on(self: &mut EmitterData, event: &str, listener: Poly, me: SelfHandle) -> Handle {
        self.push(event, listener, false, false);
        me
    }

    /// emitter.addListener(event, listener) — alias of `on`.
    #[rtse::method(name = "addListener", returns = "EventEmitter")]
    fn add_listener(self: &mut EmitterData, event: &str, listener: Poly, me: SelfHandle) -> Handle {
        self.push(event, listener, false, false);
        me
    }

    /// emitter.once(event, listener) — listener fires once, then is removed.
    #[rtse::method(name = "once", returns = "EventEmitter")]
    fn once(self: &mut EmitterData, event: &str, listener: Poly, me: SelfHandle) -> Handle {
        self.push(event, listener, true, false);
        me
    }

    /// emitter.prependListener(event, listener) — unshift to the front.
    #[rtse::method(name = "prependListener", returns = "EventEmitter")]
    fn prepend_listener(
        self: &mut EmitterData,
        event: &str,
        listener: Poly,
        me: SelfHandle,
    ) -> Handle {
        self.push(event, listener, false, true);
        me
    }

    /// emitter.prependOnceListener(event, listener).
    #[rtse::method(name = "prependOnceListener", returns = "EventEmitter")]
    fn prepend_once_listener(
        self: &mut EmitterData,
        event: &str,
        listener: Poly,
        me: SelfHandle,
    ) -> Handle {
        self.push(event, listener, true, true);
        me
    }

    /// emitter.off(event, listener) — remove by listener identity; returns `this`.
    #[rtse::method(name = "off", returns = "EventEmitter")]
    fn off(self: &mut EmitterData, event: &str, listener: Poly, me: SelfHandle) -> Handle {
        let key = listener_identity(listener);
        if let Some(list) = self.listeners.get_mut(event) {
            list.retain(|l| listener_identity(l.fn_ptr) != key);
        }
        me
    }

    /// emitter.removeListener(event, listener) — alias of `off`.
    #[rtse::method(name = "removeListener", returns = "EventEmitter")]
    fn remove_listener(self: &mut EmitterData, event: &str, listener: Poly, me: SelfHandle) -> Handle {
        let key = listener_identity(listener);
        if let Some(list) = self.listeners.get_mut(event) {
            list.retain(|l| listener_identity(l.fn_ptr) != key);
        }
        me
    }

    /// emitter.removeAllListeners(event).
    #[rtse::method(name = "removeAllListeners", returns = "EventEmitter")]
    fn remove_all_listeners(self: &mut EmitterData, event: &str, me: SelfHandle) -> Handle {
        self.listeners.remove(event);
        me
    }

    /// emitter.emit(event) — no args.
    #[rtse::method(name = "emit")]
    fn emit0(self: &EmitterData, event: &str, me: SelfHandle) -> bool {
        emit_via_handle(me, event, [undef(), undef(), undef(), undef()])
    }

    /// emitter.emit(event, a0) — the arg is forwarded to listeners unchanged.
    #[rtse::method(name = "emit")]
    fn emit1(self: &EmitterData, event: &str, a0: Poly, me: SelfHandle) -> bool {
        emit_via_handle(me, event, [a0, undef(), undef(), undef()])
    }

    /// emitter.emit(event, a0, a1).
    #[rtse::method(name = "emit")]
    fn emit2(self: &EmitterData, event: &str, a0: Poly, a1: Poly, me: SelfHandle) -> bool {
        emit_via_handle(me, event, [a0, a1, undef(), undef()])
    }

    /// emitter.emit(event, a0, a1, a2).
    #[rtse::method(name = "emit")]
    fn emit3(self: &EmitterData, event: &str, a0: Poly, a1: Poly, a2: Poly, me: SelfHandle) -> bool {
        emit_via_handle(me, event, [a0, a1, a2, undef()])
    }

    /// emitter.emitHandle(event, handle) — legacy alias forwarding the raw word.
    #[rtse::method(name = "emitHandle")]
    fn emit_handle(self: &EmitterData, event: &str, arg: U64, me: SelfHandle) -> bool {
        emit_via_handle(me, event, [arg, undef(), undef(), undef()])
    }

    /// emitter.listeners(event) — array of listener fn words.
    #[rtse::method(name = "listeners")]
    fn listeners(self: &EmitterData, event: &str) -> Vec<Handle> {
        self.listener_words(event)
    }

    /// emitter.rawListeners(event) — alias of `listeners`.
    #[rtse::method(name = "rawListeners")]
    fn raw_listeners(self: &EmitterData, event: &str) -> Vec<Handle> {
        self.listener_words(event)
    }

    /// emitter.listenerCount(event).
    #[rtse::method(name = "listenerCount")]
    fn listener_count(self: &EmitterData, event: &str) -> i64 {
        self.listeners.get(event).map(|v| v.len() as i64).unwrap_or(0)
    }

    /// emitter.eventNames().
    #[rtse::method(name = "eventNames")]
    fn event_names(self: &EmitterData) -> Vec<String> {
        self.listeners
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// emitter.getMaxListeners().
    #[rtse::method(name = "getMaxListeners")]
    fn get_max_listeners(self: &EmitterData) -> i64 {
        self.max_listeners
    }

    /// emitter.setMaxListeners(n) — returns `this`.
    #[rtse::method(name = "setMaxListeners", returns = "EventEmitter")]
    fn set_max_listeners(self: &mut EmitterData, n: i64, me: SelfHandle) -> Handle {
        self.max_listeners = n;
        me
    }
}

impl EmitterData {
    /// Add a listener word to `event` (front/back, once/not). Plain Rust helper —
    /// not an `#[rtse::*]` member (the macro leaves it untouched).
    fn push(&mut self, event: &str, word: u64, once: bool, front: bool) {
        let listener = Listener { fn_ptr: word, once };
        let list = self.listeners.entry(event.to_string()).or_default();
        if front {
            list.insert(0, listener);
        } else {
            list.push(listener);
        }
    }

    /// The listener fn words for `event`, as raw `u64`s (marshalled to a JS array
    /// by the macro's `Vec<Handle>` return path).
    fn listener_words(&self, event: &str) -> Vec<u64> {
        self.listeners
            .get(event)
            .map(|list| list.iter().map(|l| l.fn_ptr).collect())
            .unwrap_or_default()
    }
}

fn undef() -> u64 {
    POLY_UNDEFINED
}

/// Shared emit: snapshot listeners + drop `once`s UNDER the entry lock, then invoke
/// each OUTSIDE the lock forwarding the argument WORDS. `'error'` with no listener
/// throws (Node's special case). Operating on the real handle (not the macro's
/// cloned receiver) keeps reentrant `on`/`emit` from a listener correct.
fn emit_via_handle(me: u64, event: &str, args: [u64; 4]) -> bool {
    let (snapshot, async_mode) = with_rtse_mut::<EmitterData, _>(me, |data| {
        let Some(data) = data else {
            return (Vec::new(), false);
        };
        let list = data.listeners.get(event).cloned().unwrap_or_default();
        if let Some(vec) = data.listeners.get_mut(event) {
            vec.retain(|l| !l.once);
        }
        (list, data.async_mode)
    });
    if snapshot.is_empty() {
        if event == "error" {
            unsafe extern "C" {
                fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
            }
            let msg = "Unhandled 'error' event";
            unsafe {
                __rtsadp_throw_js_error(b"Error".as_ptr(), 5, msg.as_ptr(), msg.len() as i64);
            }
        }
        return false;
    }
    if async_mode {
        for listener in snapshot {
            rayon::spawn(move || invoke_listener(listener.fn_ptr, args));
        }
    } else {
        for listener in &snapshot {
            invoke_listener(listener.fn_ptr, args);
        }
    }
    true
}

/// Invoke a listener forwarding up to 4 PolyValue argument WORDS unchanged (the
/// callback bridge — a string/object/number arg keeps its tag). A listener is a
/// PolyValue FUNCTION word (new engine) → dispatched through the codegen fn-invoke
/// thunk; a legacy raw `Entry::Function` handle → the registry invoker. A named fn
/// / non-capturing arrow reifies to a raw code ptr (fast path); a capturing arrow
/// carries its env.
#[inline]
fn invoke_listener(fn_ptr: u64, args: [u64; 4]) {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_FUNCTION, POLY_TAG_SHIFT};
    let is_poly_fn = (fn_ptr & POLY_BOX_BASE) == POLY_BOX_BASE
        && ((fn_ptr >> POLY_TAG_SHIFT) & 0x7) == POLY_TAG_FUNCTION;
    if is_poly_fn {
        crate::gc_surface::__rtsadp_fn_invoke(
            fn_ptr, args[0], args[1], args[2], args[3], POLY_UNDEFINED,
        );
        return;
    }
    let is_handle = rts_engine::heap::handles::with_entry(fn_ptr, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::Function(_)))
    });
    if is_handle {
        rts_primitives::function::ops::invoke_fn_ptr_with_registry(
            fn_ptr,
            &[args[0] as i64, args[1] as i64, args[2] as i64, args[3] as i64],
        );
    }
}

/// The IDENTITY key of a listener value for `off`. Each `ee.on(ev, f)` /
/// `ee.off(ev, f)` reifies `f` to a FRESH `Entry::Function` slot, so comparing the
/// PolyValue words (or handles) directly never matches — the stable identity of a
/// named fn / non-capturing arrow is its underlying CODE POINTER. A
/// capture-carrying closure keeps the word itself (per-reify env — JS semantics:
/// two separately-created closures are distinct listeners anyway).
fn listener_identity(fn_ptr: u64) -> u64 {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_FUNCTION, POLY_TAG_SHIFT};
    let is_poly_fn = (fn_ptr & POLY_BOX_BASE) == POLY_BOX_BASE
        && ((fn_ptr >> POLY_TAG_SHIFT) & 0x7) == POLY_TAG_FUNCTION;
    let handle = if is_poly_fn {
        rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(fn_ptr & 0xFFFF_FFFF_FFFF)
    } else {
        fn_ptr
    };
    rts_engine::heap::handles::with_entry(handle, |e| match e {
        Some(rts_engine::heap::handles::Entry::Function(f)) if f.bound_args.is_empty() => {
            Some(f.fn_ptr)
        }
        _ => None,
    })
    .unwrap_or(fn_ptr)
}

/// Registra a classe global `EventEmitter` no motor. Thin wrapper sobre o
/// `register` gerado pela macro `#[rtse::class]` — mantém o nome que o
/// `registry_build.rs` já referencia (`register_class_spec`).
pub fn register_class_spec(e: &mut rts_engine::Engine) {
    register(e);
}
