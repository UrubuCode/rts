use crate::namespaces::gc::handles::{alloc_entry, shard_for_handle, Entry};

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
    let bytes = {
        let guard = shard_for_handle(buf_handle).lock().unwrap();
        match guard.get(buf_handle) {
            Some(Entry::Buffer(v)) | Some(Entry::String(v)) => v.clone(),
            _ => return 0,
        }
    };
    alloc_entry(Entry::String(bytes))
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_STRUCTURED_CLONE(handle: u64) -> u64 {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::String(v)) => {
            let cloned = v.clone();
            drop(guard);
            alloc_entry(Entry::String(cloned))
        }
        Some(Entry::Buffer(v)) => {
            let cloned = v.clone();
            drop(guard);
            alloc_entry(Entry::Buffer(cloned))
        }
        Some(Entry::Vec(v)) => {
            let cloned = v.as_ref().clone();
            drop(guard);
            alloc_entry(Entry::Vec(Box::new(cloned)))
        }
        Some(Entry::Map(m)) => {
            let cloned = m.as_ref().clone();
            drop(guard);
            alloc_entry(Entry::Map(Box::new(cloned)))
        }
        Some(Entry::Json(j)) => {
            let cloned = j.as_ref().clone();
            drop(guard);
            alloc_entry(Entry::Json(Box::new(cloned)))
        }
        // Primitivos (número, bool): caller já tem o valor direto, não handle
        _ => handle,
    }
}

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(fp: u64) {
    if fp != 0 {
        unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
    }
}

// TextEncoder / TextDecoder constructors — stateless, token handle.
// encode/decode são chamados com (self_handle, str_ptr, str_len) no instance path
// mas o self é ignorado; a impl real está em ENCODE/DECODE acima.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_NEW() -> u64 {
    alloc_entry(Entry::Env(vec![1])) // token "TextEncoder"
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_NEW() -> u64 {
    alloc_entry(Entry::Env(vec![2])) // token "TextDecoder"
}

// Instance method variants: (self_handle, ptr, len) — self ignored.
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
