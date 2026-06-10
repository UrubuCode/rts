//! `EventEmitter` global class — hybrid sync/async listener dispatch.
//!
//! Migrado ao modelo `#[rts_class]` (stage 5; prefixo de simbolo `EE`, spec
//! `CLASS_SPEC`). `EmitterData`/`Listener` + os helpers `clone_arc`/`with_emitter`
//! ficam como itens de modulo. Listener signature: `extern "C" fn(f64) -> f64`.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rts_engine::abi::ty::{Bool, Handle, I64, U64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, free_handle, with_entry, Entry};

#[derive(Clone)]
struct Listener {
    fn_ptr: u64,
    once: bool,
}

pub struct EmitterData {
    listeners: HashMap<String, Vec<Listener>>,
    async_mode: bool,
}

impl EmitterData {
    fn new(async_mode: bool) -> Self {
        Self {
            listeners: HashMap::new(),
            async_mode,
        }
    }
}

fn clone_arc(handle: u64) -> Option<Arc<Mutex<dyn Any + Send>>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::EventEmitter(arc)) => Some(arc.clone()),
        _ => None,
    })
}

fn with_emitter<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut EmitterData) -> R,
{
    if let Some(arc) = clone_arc(handle) {
        let mut any = arc.lock().unwrap();
        if let Some(data) = any.downcast_mut::<EmitterData>() {
            return f(data);
        }
    }
    default
}

/// EventEmitter — on/once/off + emit (sync ou async fire-and-forget).
#[rts_class(EventEmitter, prefix = "EE", spec = "CLASS_SPEC")]
impl EventEmitterClass {
    /// emitter.free() — libera o handle.
    #[rts_method(ts = "free(): number")]
    pub fn free(handle: Handle) -> I64 {
        if free_handle(handle) {
            1
        } else {
            0
        }
    }

    /// new EventEmitter()
    #[rts_ctor(ts = "new EventEmitter(): EventEmitter")]
    pub fn new() -> Handle {
        let data: Arc<Mutex<dyn Any + Send>> = Arc::new(Mutex::new(EmitterData::new(false)));
        alloc_entry(Entry::EventEmitter(data))
    }

    /// new EventEmitter(async)
    #[rts_ctor(
        symbol = "__RTS_FN_GL_EE_NEW_ASYNC",
        ts = "new EventEmitter(async: boolean): EventEmitter"
    )]
    pub fn new_async(is_async: Bool) -> Handle {
        let data: Arc<Mutex<dyn Any + Send>> =
            Arc::new(Mutex::new(EmitterData::new(is_async != 0)));
        alloc_entry(Entry::EventEmitter(data))
    }

    /// emitter.on(event, listener)
    #[rts_method(
        ts = "on(event: string, listener: (arg: number) => void): this",
        opt_str
    )]
    pub fn on(handle: Handle, event: Str, fn_ptr: U64) -> Handle {
        let event = event.unwrap_or("").to_string();
        with_emitter(handle, handle, |data| {
            data.listeners.entry(event).or_default().push(Listener {
                fn_ptr,
                once: false,
            });
            handle
        })
    }

    /// emitter.once(event, listener)
    #[rts_method(
        ts = "once(event: string, listener: (arg: number) => void): this",
        opt_str
    )]
    pub fn once(handle: Handle, event: Str, fn_ptr: U64) -> Handle {
        let event = event.unwrap_or("").to_string();
        with_emitter(handle, handle, |data| {
            data.listeners
                .entry(event)
                .or_default()
                .push(Listener { fn_ptr, once: true });
            handle
        })
    }

    /// emitter.off(event, listener)
    #[rts_method(
        ts = "off(event: string, listener: (arg: number) => void): this",
        opt_str
    )]
    pub fn off(handle: Handle, event: Str, fn_ptr: U64) -> Handle {
        let event = event.unwrap_or("").to_string();
        with_emitter(handle, handle, |data| {
            if let Some(list) = data.listeners.get_mut(&event) {
                list.retain(|l| l.fn_ptr != fn_ptr);
            }
            handle
        })
    }

    /// emitter.emit(event, arg) — listeners recebem `arg` como number (f64).
    #[rts_method(ts = "emit(event: string, arg: number): boolean", opt_str)]
    pub fn emit(handle: Handle, event: Str, arg: I64) -> Bool {
        let event = event.unwrap_or("").to_string();
        let (snapshot, async_mode) = {
            let Some(arc) = clone_arc(handle) else {
                return 0;
            };
            let mut any = arc.lock().unwrap();
            let Some(data) = any.downcast_mut::<EmitterData>() else {
                return 0;
            };
            let list = data.listeners.get(&event).cloned().unwrap_or_default();
            if let Some(vec) = data.listeners.get_mut(&event) {
                vec.retain(|l| !l.once);
            }
            (list, data.async_mode)
        };
        if snapshot.is_empty() {
            return 0;
        }
        let arg_f64 = arg as f64;
        if async_mode {
            for listener in snapshot {
                rayon::spawn(move || {
                    let f: extern "C" fn(f64) -> f64 =
                        unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                    f(arg_f64);
                });
            }
        } else {
            for listener in &snapshot {
                let f: extern "C" fn(f64) -> f64 =
                    unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                f(arg_f64);
            }
        }
        1
    }

    /// emitter.emitHandle(event, handle) — passa o arg como bits f64 (handle raw).
    #[rts_method(
        name = "emitHandle",
        ts = "emitHandle(event: string, handle: number): boolean",
        opt_str
    )]
    pub fn emit_handle(handle: Handle, event: Str, arg: I64) -> Bool {
        let event = event.unwrap_or("").to_string();
        let (snapshot, async_mode) = {
            let Some(arc) = clone_arc(handle) else {
                return 0;
            };
            let mut any = arc.lock().unwrap();
            let Some(data) = any.downcast_mut::<EmitterData>() else {
                return 0;
            };
            let list = data.listeners.get(&event).cloned().unwrap_or_default();
            if let Some(vec) = data.listeners.get_mut(&event) {
                vec.retain(|l| !l.once);
            }
            (list, data.async_mode)
        };
        if snapshot.is_empty() {
            return 0;
        }
        let arg_bits = f64::from_bits(arg as u64);
        if async_mode {
            for listener in snapshot {
                rayon::spawn(move || {
                    let f: extern "C" fn(f64) -> f64 =
                        unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                    f(arg_bits);
                });
            }
        } else {
            for listener in &snapshot {
                let f: extern "C" fn(f64) -> f64 =
                    unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                f(arg_bits);
            }
        }
        1
    }

    /// emitter.removeAllListeners(event)
    #[rts_method(
        name = "removeAllListeners",
        symbol = "__RTS_FN_GL_EE_REMOVE_ALL",
        ts = "removeAllListeners(event: string): this",
        opt_str
    )]
    pub fn remove_all_listeners(handle: Handle, event: Str) -> Handle {
        let event = event.unwrap_or("").to_string();
        with_emitter(handle, handle, |data| {
            data.listeners.remove(&event);
            handle
        })
    }

    /// emitter.listenerCount(event)
    #[rts_method(
        name = "listenerCount",
        ts = "listenerCount(event: string): number",
        opt_str,
        pure
    )]
    pub fn listener_count(handle: Handle, event: Str) -> I64 {
        let event = event.unwrap_or("").to_string();
        with_emitter(handle, 0, |data| {
            data.listeners
                .get(&event)
                .map(|v| v.len() as i64)
                .unwrap_or(0)
        })
    }

    /// emitter.eventNames()
    #[rts_method(name = "eventNames", ts = "eventNames(): string[]", pure)]
    pub fn event_names(handle: Handle) -> Handle {
        let names: Vec<String> = {
            let Some(arc) = clone_arc(handle) else {
                return 0;
            };
            let any = arc.lock().unwrap();
            let Some(data) = any.downcast_ref::<EmitterData>() else {
                return 0;
            };
            data.listeners
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, _)| k.clone())
                .collect()
        };
        let str_handles: Vec<i64> = names
            .into_iter()
            .map(|name| alloc_entry(Entry::String(name.into_bytes())) as i64)
            .collect();
        alloc_entry(Entry::Vec(Box::new(str_handles)))
    }
}
