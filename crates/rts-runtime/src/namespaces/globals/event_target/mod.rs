//! `EventTarget` + `Event` global classes (#63).
//!
//! EventTarget: addEventListener(type, fn), removeEventListener, dispatchEvent.
//! Event: new Event(type) com .type readonly. Migrado ao modelo `#[rts_class]`
//! (stage 5) — duas classes no mesmo arquivo (members inline na spec).

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

// ── EventTarget ───────────────────────────────────────────────────────────────

/// EventTarget — addEventListener / removeEventListener / dispatchEvent.
#[rts_class(EventTarget, prefix = "EVENT_TARGET", spec = "EVENT_TARGET_CLASS_SPEC")]
impl EventTargetClass {
    /// new EventTarget()
    #[rts_ctor(ts = "new EventTarget()", pure)]
    pub fn new() -> Handle {
        let listeners = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        let mut m: IndexMap<String, i64> = IndexMap::new();
        m.insert("listeners".to_string(), listeners as i64);
        m.insert("__rts_class".to_string(), {
            alloc_entry(Entry::String(b"EventTarget".to_vec())) as i64
        });
        alloc_entry(Entry::Map(Box::new(m)))
    }

    /// addEventListener(type, listener)
    #[rts_method(
        name = "addEventListener",
        symbol = "__RTS_FN_GL_EVENT_TARGET_ADD_LISTENER",
        ts = "addEventListener(type: string, listener: (ev: Event) => void): void",
        opt_str
    )]
    pub fn add_event_listener(h: Handle, ty_s: Str, fn_h: Handle) {
        let ty = ty_s.unwrap_or("").to_string();
        let listeners_h: u64 = with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
            _ => 0,
        });
        if listeners_h == 0 {
            return;
        }
        let mut vec_h: u64 = with_entry(listeners_h, |e| match e {
            Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
            _ => 0,
        });
        if vec_h == 0 {
            vec_h = alloc_entry(Entry::Vec(Box::new(Vec::new())));
            with_entry_mut(listeners_h, |e| {
                if let Some(Entry::Map(m)) = e {
                    m.insert(ty.clone(), vec_h as i64);
                }
            });
        }
        with_entry_mut(vec_h, |e| {
            if let Some(Entry::Vec(v)) = e {
                v.push(fn_h as i64);
            }
        });
    }

    /// removeEventListener(type, listener)
    #[rts_method(
        name = "removeEventListener",
        symbol = "__RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER",
        ts = "removeEventListener(type: string, listener: (ev: Event) => void): void",
        opt_str
    )]
    pub fn remove_event_listener(h: Handle, ty_s: Str, fn_h: Handle) {
        let ty = ty_s.unwrap_or("").to_string();
        let listeners_h: u64 = with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
            _ => 0,
        });
        if listeners_h == 0 {
            return;
        }
        let vec_h: u64 = with_entry(listeners_h, |e| match e {
            Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
            _ => 0,
        });
        if vec_h == 0 {
            return;
        }
        with_entry_mut(vec_h, |e| {
            if let Some(Entry::Vec(v)) = e {
                v.retain(|&x| x != fn_h as i64);
            }
        });
    }

    /// dispatchEvent(event) — chama listeners do type, retorna bool.
    #[rts_method(
        name = "dispatchEvent",
        symbol = "__RTS_FN_GL_EVENT_TARGET_DISPATCH",
        ts = "dispatchEvent(event: Event): boolean"
    )]
    pub fn dispatch_event(h: Handle, event_h: Handle) -> Bool {
        let ty: String = with_entry(event_h, |e| match e {
            Some(Entry::Map(m)) => {
                let t_h = m.get("type").copied().unwrap_or(0) as u64;
                with_entry(t_h, |te| match te {
                    Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                    _ => String::new(),
                })
            }
            _ => String::new(),
        });
        if ty.is_empty() {
            return 1;
        }
        let listeners_h: u64 = with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
            _ => 0,
        });
        let vec_h: u64 = with_entry(listeners_h, |e| match e {
            Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
            _ => 0,
        });
        let fps: Vec<u64> = with_entry(vec_h, |e| match e {
            Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
            _ => Vec::new(),
        });
        for fp in fps {
            if fp == 0 {
                continue;
            }
            let args = alloc_entry(Entry::Vec(Box::new(vec![event_h as i64])));
            unsafe extern "C" {
                fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
            }
            let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(fp as i64, h as i64, args) };
        }
        1
    }
}

// ── Event ─────────────────────────────────────────────────────────────────────

/// Event — new Event(type) com `.type` readonly.
#[rts_class(Event, prefix = "EVENT", spec = "EVENT_CLASS_SPEC")]
impl EventClass {
    /// new Event(type)
    #[rts_ctor(ts = "new Event(type: string)", opt_str, pure)]
    pub fn new(ty_s: Str) -> Handle {
        let ty = ty_s.unwrap_or("");
        let type_h = alloc_entry(Entry::String(ty.as_bytes().to_vec()));
        let mut m: IndexMap<String, i64> = IndexMap::new();
        m.insert("type".to_string(), type_h as i64);
        m.insert("__rts_class".to_string(), {
            alloc_entry(Entry::String(b"Event".to_vec())) as i64
        });
        alloc_entry(Entry::Map(Box::new(m)))
    }

    /// event.type — readonly.
    #[rts_getter(
        name = "type",
        symbol = "__RTS_FN_GL_EVENT_TYPE",
        ts = "readonly type: string",
        pure
    )]
    pub fn event_type(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("type").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }
}
