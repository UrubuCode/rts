//! Instance methods adicionados pela GlobalClassSpec de String.
//!
//! Metodos novos que nao existiam no namespace rts:string original.
//! Simbolos usam prefixo __RTS_FN_GL_STRING_*.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64);
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// String.fromCharCode(code) — retorna string de 1 char a partir de code point Unicode.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CHAR_CODE(code: i64) -> u64 {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    alloc_str(s)
}

/// str.slice(start, end) — fatiamento por indice de char Unicode.
/// Indices negativos contam do fim. Retorna handle para nova string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SLICE(
    ptr: *const u8,
    len: i64,
    start: i64,
    end: i64,
) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return alloc_str("");
    };
    let char_count = s.chars().count() as i64;
    let norm = |i: i64| -> usize {
        let n = if i < 0 { char_count + i } else { i };
        n.clamp(0, char_count) as usize
    };
    let s_idx = norm(start);
    let e_idx = norm(end);
    if s_idx >= e_idx {
        return alloc_str("");
    }
    let result: String = s.chars().skip(s_idx).take(e_idx - s_idx).collect();
    alloc_str(&result)
}

/// str.split(sep) — retorna Vec handle com os segmentos como string handles (i64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT(
    s_ptr: *const u8,
    s_len: i64,
    sep_ptr: *const u8,
    sep_len: i64,
) -> u64 {
    let s = str_from_abi(s_ptr, s_len).unwrap_or("");
    let sep = str_from_abi(sep_ptr, sep_len).unwrap_or("");
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    for part in s.split(sep) {
        let h = alloc_str(part) as i64;
        unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
    }
    vec_h
}

/// str.toString() / str.valueOf() — identidade, retorna o proprio handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_STRING(handle: u64) -> u64 {
    handle
}

/// str.padStart(targetLen, padStr) — preenche inicio com padStr ate targetLen chars.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_START(
    ptr: *const u8,
    len: i64,
    target_len: i64,
    pad_ptr: *const u8,
    pad_len: i64,
) -> u64 {
    let s = str_from_abi(ptr, len).unwrap_or("");
    let pad = str_from_abi(pad_ptr, pad_len).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || pad.is_empty() {
        return alloc_str(s);
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = pad.chars().collect();
    let prefix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{prefix}{s}"))
}

/// str.padEnd(targetLen, padStr) — preenche fim com padStr ate targetLen chars.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_END(
    ptr: *const u8,
    len: i64,
    target_len: i64,
    pad_ptr: *const u8,
    pad_len: i64,
) -> u64 {
    let s = str_from_abi(ptr, len).unwrap_or("");
    let pad = str_from_abi(pad_ptr, pad_len).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || pad.is_empty() {
        return alloc_str(s);
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = pad.chars().collect();
    let suffix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{s}{suffix}"))
}
