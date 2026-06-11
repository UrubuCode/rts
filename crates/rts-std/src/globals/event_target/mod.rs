//! `EventTarget` + `Event` global classes (#63).
//!
//! EventTarget: addEventListener(type, fn), removeEventListener, dispatchEvent.
//! Event: new Event(type) com .type readonly.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_EVENT_TARGET_*` / `__RTS_FN_GL_EVENT_*` +
//! `register_event_target_class_spec()` / `register_event_class_spec()` são
//! escritos à mão (duas classes no mesmo arquivo).

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

// ── EventTarget ───────────────────────────────────────────────────────────────

/// new EventTarget()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_NEW() -> Handle {
    let listeners = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("listeners".to_string(), listeners as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"EventTarget".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

/// addEventListener(type, listener)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_ADD_LISTENER(
    h: Handle,
    ty_s_ptr: *const u8,
    ty_s_len: i64,
    fn_h: Handle,
) {
    let ty_s = unsafe { rts_engine::abi::str_abi::from_abi(ty_s_ptr, ty_s_len) };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER(
    h: Handle,
    ty_s_ptr: *const u8,
    ty_s_len: i64,
    fn_h: Handle,
) {
    let ty_s = unsafe { rts_engine::abi::str_abi::from_abi(ty_s_ptr, ty_s_len) };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_DISPATCH(h: Handle, event_h: Handle) -> Bool {
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

// ── Event ─────────────────────────────────────────────────────────────────────

/// new Event(type)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_NEW(ty_s_ptr: *const u8, ty_s_len: i64) -> Handle {
    let ty_s = unsafe { rts_engine::abi::str_abi::from_abi(ty_s_ptr, ty_s_len) };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TYPE(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("type").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

// ── Builders hand-written das classes globais `EventTarget` + `Event` ──────────

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `EventTarget` no motor (hand-written, sem macro).
pub fn register_event_target_class_spec(e: &mut Engine) {
    e.class("EventTarget")
        .doc("EventTarget — addEventListener / removeEventListener / dispatchEvent.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_EVENT_TARGET_NEW",
            "new EventTarget()",
            "new EventTarget()",
            __RTS_FN_GL_EVENT_TARGET_NEW as *const u8,
            true,
        ))
        .member(m(
            "addEventListener",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Void,
            ),
            "__RTS_FN_GL_EVENT_TARGET_ADD_LISTENER",
            "addEventListener(type: string, listener: (ev: Event) => void): void",
            "addEventListener(type, listener)",
            __RTS_FN_GL_EVENT_TARGET_ADD_LISTENER as *const u8,
            false,
        ))
        .member(m(
            "removeEventListener",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Void,
            ),
            "__RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER",
            "removeEventListener(type: string, listener: (ev: Event) => void): void",
            "removeEventListener(type, listener)",
            __RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER as *const u8,
            false,
        ))
        .member(m(
            "dispatchEvent",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_EVENT_TARGET_DISPATCH",
            "dispatchEvent(event: Event): boolean",
            "dispatchEvent(event) — chama listeners do type, retorna bool.",
            __RTS_FN_GL_EVENT_TARGET_DISPATCH as *const u8,
            false,
        ))
        .done();
}

/// Registra a classe global `Event` no motor (hand-written, sem macro).
pub fn register_event_class_spec(e: &mut Engine) {
    e.class("Event")
        .doc("Event — new Event(type) com `.type` readonly.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_EVENT_NEW",
            "new Event(type: string)",
            "new Event(type)",
            __RTS_FN_GL_EVENT_NEW as *const u8,
            true,
        ))
        .member(m(
            "type",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_EVENT_TYPE",
            "readonly type: string",
            "event.type — readonly.",
            __RTS_FN_GL_EVENT_TYPE as *const u8,
            true,
        ))
        .done();
}
