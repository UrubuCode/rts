use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

fn str_from_parts(ptr: i64, len: i64) -> &'static str {
    if ptr == 0 || len == 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ENCODE(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    alloc_entry(Entry::Buffer(s.as_bytes().to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_DECODE(buf_handle: u64) -> u64 {
    let bytes = with_entry(buf_handle, |entry| match entry {
        Some(Entry::Buffer(v)) | Some(Entry::String(v)) => Some(v.clone()),
        _ => None,
    });
    match bytes {
        Some(b) => alloc_entry(Entry::String(b)),
        None => 0,
    }
}

const B64_ALPHA: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let (b0, b1, b2) = (bytes[i], bytes[i + 1], bytes[i + 2]);
        out.push(B64_ALPHA[(b0 >> 2) as usize]);
        out.push(B64_ALPHA[(((b0 & 3) << 4) | (b1 >> 4)) as usize]);
        out.push(B64_ALPHA[(((b1 & 15) << 2) | (b2 >> 6)) as usize]);
        out.push(B64_ALPHA[(b2 & 63) as usize]);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let b0 = bytes[i];
            out.push(B64_ALPHA[(b0 >> 2) as usize]);
            out.push(B64_ALPHA[((b0 & 3) << 4) as usize]);
            out.push(b'=');
            out.push(b'=');
        }
        2 => {
            let (b0, b1) = (bytes[i], bytes[i + 1]);
            out.push(B64_ALPHA[(b0 >> 2) as usize]);
            out.push(B64_ALPHA[(((b0 & 3) << 4) | (b1 >> 4)) as usize]);
            out.push(B64_ALPHA[((b1 & 15) << 2) as usize]);
            out.push(b'=');
        }
        _ => {}
    }
    out
}

fn b64_decode(s: &[u8]) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let s: Vec<u8> = s.iter().copied().filter(|&c| c != b'\n' && c != b'\r').collect();
    if s.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut i = 0;
    while i < s.len() {
        let a = val(s[i])?;
        let b = val(s[i + 1])?;
        let c = val(s[i + 2])?;
        let d = val(s[i + 3])?;
        out.push((a << 2) | (b >> 4));
        if s[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if s[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Some(out)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_BTOA(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    let encoded = b64_encode(s.as_bytes());
    alloc_entry(Entry::String(encoded))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ATOB(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    match b64_decode(s.as_bytes()) {
        Some(decoded) => alloc_entry(Entry::String(decoded)),
        None => 0,
    }
}

/// (#316) Helper recursivo: clona handle preservando set_kind/map_kind
/// flags. Slots que parecem handles validos sao clonados recursivamente.
/// `visited` mapeia handle_original -> handle_clone para suportar
/// self-references (JS spec do structuredClone preserva ciclos).
fn clone_handle_deep(
    handle: u64,
    visited: &mut std::collections::HashMap<u64, u64>,
) -> u64 {
    if let Some(&existing) = visited.get(&handle) {
        return existing;
    }
    let entry_clone = with_entry(handle, |entry| match entry {
        Some(Entry::String(v)) => Some(Entry::String(v.clone())),
        Some(Entry::Buffer(v)) => Some(Entry::Buffer(v.clone())),
        Some(Entry::Vec(v)) => Some(Entry::Vec(v.clone())),
        Some(Entry::Map(m)) => Some(Entry::Map(m.clone())),
        Some(Entry::Json(j)) => Some(Entry::Json(j.clone())),
        Some(Entry::DateMs(ms)) => Some(Entry::DateMs(*ms)),
        // Regex nao tem Clone — passa handle original (shared, imutavel).
        _ => None,
    });
    let Some(entry) = entry_clone else { return handle; };
    let new_h = alloc_entry(entry);
    visited.insert(handle, new_h);
    // Preserva kind flags
    if crate::namespaces::collections::map::handle_is_set_kind(handle) {
        crate::namespaces::collections::map::mark_set_kind(new_h);
    }
    // Deep clone de slots que sao handles a estruturas clonaveis.
    use crate::namespaces::gc::handles::with_entry_mut;
    let _ = with_entry_mut(new_h, |entry| match entry {
        Some(Entry::Map(m)) => {
            let pairs: Vec<(String, i64)> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
            for (k, v) in pairs {
                let v_u = v as u64;
                if v_u > 0xFFFF_FFFF {
                    let v_kind = with_entry(v_u, |e| matches!(
                        e,
                        Some(Entry::Map(_)) | Some(Entry::Vec(_)) | Some(Entry::String(_))
                        | Some(Entry::Buffer(_)) | Some(Entry::Json(_))
                        | Some(Entry::DateMs(_)) | Some(Entry::Regex(_))
                    ));
                    if v_kind {
                        let cloned = clone_handle_deep(v_u, visited);
                        m.insert(k, cloned as i64);
                    }
                }
            }
        }
        Some(Entry::Vec(v)) => {
            for slot in v.iter_mut() {
                let s_u = *slot as u64;
                if s_u > 0xFFFF_FFFF {
                    let v_kind = with_entry(s_u, |e| matches!(
                        e,
                        Some(Entry::Map(_)) | Some(Entry::Vec(_)) | Some(Entry::String(_))
                        | Some(Entry::Buffer(_)) | Some(Entry::Json(_))
                        | Some(Entry::DateMs(_)) | Some(Entry::Regex(_))
                    ));
                    if v_kind {
                        *slot = clone_handle_deep(s_u, visited) as i64;
                    }
                }
            }
        }
        _ => {}
    });
    new_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_STRUCTURED_CLONE(handle: u64) -> u64 {
    let mut visited = std::collections::HashMap::new();
    clone_handle_deep(handle, &mut visited)
}

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

/// (cross-runtime #56) Microtask queue thread-local. queueMicrotask
/// enfileira o callback; ele eh drenado no fim do task corrente (top-level
/// __RTS_MAIN ou apos um await).
use std::cell::RefCell;
thread_local! {
    static MICROTASK_QUEUE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(fp: u64) {
    // (cross-runtime #285) JS spec quer enfileiramento, mas como Promise.then
    // de promises ja' settled executa sync inline em RTS (PROMISE_THEN2 fast-path),
    // queueMicrotask tambem precisa executar inline pra preservar a ordem FIFO
    // entre eles. Refator completo precisa de event loop real (#376/#207).
    if fp != 0 {
        unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0); }
    }
}

/// Drena microtasks pendentes. Chamada pelo pipeline pos-main e tambem
/// pode ser chamada pelo codegen no fim de cada task (futuro).
pub fn drain_microtasks() {
    loop {
        let queue: Vec<u64> = MICROTASK_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if queue.is_empty() {
            break;
        }
        for fp in queue {
            if fp != 0 {
                unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0); }
            }
        }
    }
}

// TextEncoder / TextDecoder constructors — stateless, token handle.
// encode/decode são chamados com (self_handle, str_ptr, str_len) no instance path
// mas o self é ignorado; a impl real está em ENCODE/DECODE acima.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_NEW() -> u64 {
    alloc_entry(Entry::Env(vec![1])) // token "TextEncoder"
}

/// (cross-runtime #874) Aceita label opcional como (ptr, len). Em RTS so'
/// UTF-8 e' suportado; o label e' aceito mas ignorado (Bun/Node aceitam
/// `new TextDecoder("utf-8")` sem erro).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_NEW(_label_ptr: i64, _label_len: i64) -> u64 {
    alloc_entry(Entry::Env(vec![2])) // token "TextDecoder"
}

// Instance method variants: (self_handle, ptr, len) — self ignored.
// Usados pelos GlobalClassSpec (encode/decode em receiver this).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ENCODE_INSTANCE(
    _self_h: u64,
    ptr: i64,
    len: i64,
) -> u64 {
    __RTS_FN_GL_TEXTENC_ENCODE(ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_DECODE_INSTANCE(_self_h: u64, buf_h: u64) -> u64 {
    __RTS_FN_GL_TEXTENC_DECODE(buf_h)
}
