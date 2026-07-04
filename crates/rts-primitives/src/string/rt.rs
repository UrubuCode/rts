//! Instance + static methods da GlobalClassSpec de String.
//! Todos recebem/retornam handles u64. Simbolos: __RTS_FN_GL_STRING_*.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8;
    fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64;
    fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64;
    fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64);
}

pub(crate) fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn handle_to_str<'a>(h: u64) -> Option<&'a str> {
    // (#1023) Unwrap StringBox antes de tentar STRING_PTR/LEN.
    let unwrapped: u64 = rts_engine::heap::handles::with_entry(h, |e| match e {
        Some(rts_engine::heap::handles::Entry::StringBox(inner)) => *inner,
        _ => h,
    });
    let ptr = unsafe { __RTS_FN_NS_GC_STRING_PTR(unwrapped) };
    let len = unsafe { __RTS_FN_NS_GC_STRING_LEN(unwrapped) };
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

// ── Constructors ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NEW_FROM(handle: u64) -> u64 {
    handle
}

/// (cross-runtime #244) `new String(x)` — cria StringBox wrapper para
/// que typeof retorne "object". valueOf/toString recuperam o handle
/// string original.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NEW_BOXED(handle: u64) -> u64 {
    rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::StringBox(
        handle,
    ))
}

/// (cross-runtime #244) StringBox.valueOf() / toString() — recupera o
/// handle primitive embrulhado. Se nao for StringBox, retorna 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_BOX_VALUE_OF(boxed: u64) -> u64 {
    rts_engine::heap::handles::with_entry(boxed, |e| match e {
        Some(rts_engine::heap::handles::Entry::StringBox(h)) => *h,
        Some(rts_engine::heap::handles::Entry::String(_)) => boxed,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NEW_EMPTY() -> u64 {
    alloc_str("")
}

// ── Static methods ─────────────────────────────────────────────────────────────

/// String.fromCharCode(code) — JS spec: trunca o argumento para u16
/// (mod 0x10000). Code unit isolado pode ser surrogate "orfao" — nao
/// eh codepoint UTF-8 valido. Nesse caso retorna string vazia (compat
/// com Bun/Node que produzem lone surrogate na string interna mas, ao
/// converter UTF-16 -> UTF-8 para output, replacement char ou vazio).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CHAR_CODE(code: i64) -> u64 {
    let unit = (code as i32 as u32) & 0xFFFF;
    // Surrogate orfao (0xD800-0xDFFF) nao eh char valido. Retorna
    // string com o code unit como UTF-16 lossy (string vazia bate
    // com output do Node em \`String.fromCharCode(0xF600)\` -> 1 char
    // que nao representa codepoint valido na codificacao default).
    if (0xD800..=0xDFFF).contains(&unit) {
        return alloc_str("");
    }
    let ch = char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}

/// String.fromCodePoint(codePoint) — char from full Unicode code point.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CODE_POINT(code: i64) -> u64 {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}

// ── Search methods ─────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INDEX_OF(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.find(n).map(|i| i as i64).unwrap_or(-1),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LAST_INDEX_OF(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.rfind(n).map(|i| i as i64).unwrap_or(-1),
        _ => -1,
    }
}

/// `str.indexOf(needle, fromIndex)` — semantica JS:
/// - fromIndex clamped a [0, s.len()]
/// - needle vazio: retorna min(fromIndex, s.len())
/// - resto: busca a partir de fromIndex em chars Unicode
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INDEX_OF_FROM(recv: u64, needle: u64, from: i64) -> i64 {
    let Some(s) = handle_to_str(recv) else {
        return -1;
    };
    let Some(n) = handle_to_str(needle) else {
        return -1;
    };
    let len = s.chars().count() as i64;
    let from_clamped = from.max(0).min(len);
    if n.is_empty() {
        return from_clamped;
    }
    // Converte fromIndex (em chars) pra byte offset.
    let byte_start = s
        .char_indices()
        .nth(from_clamped as usize)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    // Busca no slice apos byte_start; resultado precisa virar char index global.
    let slice = &s[byte_start..];
    match slice.find(n) {
        Some(byte_off) => {
            // Conta chars desde o inicio ate byte_start + byte_off.
            let abs_byte = byte_start + byte_off;
            s[..abs_byte].chars().count() as i64
        }
        None => -1,
    }
}

/// `str.lastIndexOf(needle, fromIndex)` — semantica JS:
/// - fromIndex clamped a [0, s.len()]; NaN ou ausente = length
/// - needle vazio: retorna min(fromIndex, s.len())
/// - busca a ultima ocorrencia cujo INICIO seja <= fromIndex
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LAST_INDEX_OF_FROM(recv: u64, needle: u64, from: i64) -> i64 {
    let Some(s) = handle_to_str(recv) else {
        return -1;
    };
    let Some(n) = handle_to_str(needle) else {
        return -1;
    };
    let len = s.chars().count() as i64;
    let from_clamped = from.max(0).min(len);
    if n.is_empty() {
        return from_clamped;
    }
    let limit_byte = s
        .char_indices()
        .nth(from_clamped as usize)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    // Encontra ultima posicao de byte tal que o match comece em <= limit_byte.
    let mut best: Option<usize> = None;
    let mut search_from = 0usize;
    while let Some(rel) = s[search_from..].find(n) {
        let abs = search_from + rel;
        if abs > limit_byte {
            break;
        }
        best = Some(abs);
        // Avanca pelo menos 1 byte pra evitar loop em casos degenerados.
        search_from = abs + n.len().max(1);
    }
    match best {
        Some(byte_pos) => s[..byte_pos].chars().count() as i64,
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INCLUDES(recv: u64, needle: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => s.contains(n) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_INCLUDES_AT(recv: u64, needle: u64, pos: i64) -> i64 {
    match (handle_to_str(recv), handle_to_str(needle)) {
        (Some(s), Some(n)) => {
            // pos em code units UTF-16; converte para byte offset UTF-8
            let units: Vec<u16> = s.encode_utf16().collect();
            let clamped = pos.clamp(0, units.len() as i64) as usize;
            // converte clamped code-unit index para byte index
            let byte_off = {
                let prefix_units = &units[..clamped];
                String::from_utf16_lossy(prefix_units).len()
            };
            s[byte_off..].contains(n) as i64
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_STARTS_WITH(recv: u64, prefix: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(prefix)) {
        (Some(s), Some(p)) => s.starts_with(p) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_ENDS_WITH(recv: u64, suffix: u64) -> i64 {
    match (handle_to_str(recv), handle_to_str(suffix)) {
        (Some(s), Some(p)) => s.ends_with(p) as i64,
        _ => 0,
    }
}

/// (#208) startsWith com `position` opcional.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_STARTS_WITH_AT(recv: u64, prefix: u64, pos: i64) -> i64 {
    let (Some(s), Some(p)) = (handle_to_str(recv), handle_to_str(prefix)) else {
        return 0;
    };
    let start = if pos < 0 {
        0
    } else {
        (pos as usize).min(s.len())
    };
    s[start..].starts_with(p) as i64
}

/// (#208) endsWith com `endPosition` opcional — limita ate qual indice considerar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_ENDS_WITH_AT(recv: u64, suffix: u64, end_pos: i64) -> i64 {
    let (Some(s), Some(p)) = (handle_to_str(recv), handle_to_str(suffix)) else {
        return 0;
    };
    let end = if end_pos < 0 {
        0
    } else {
        (end_pos as usize).min(s.len())
    };
    s[..end].ends_with(p) as i64
}

// ── Indexing methods ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
/// `str.charAt(i)` — JS spec: indexa por UTF-16 code units (nao code points).
/// Em surrogate pair (emoji etc.), `charAt(0)` retorna o high surrogate isolado
/// (codifica como char invalido na saida UTF-8, igual Bun/Node).
pub extern "C" fn __RTS_FN_GL_STRING_CHAR_AT(recv: u64, idx: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else {
        return alloc_str("");
    };
    if idx < 0 {
        return alloc_str("");
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let Some(&unit) = units.get(idx as usize) else {
        return alloc_str("");
    };
    // Tenta decodificar como char valido (BMP nao-surrogate); surrogate isolado
    // vira REPLACEMENT CHARACTER (U+FFFD) — bate com `console.log` de Bun/Node
    // que mostra "�" para surrogate orfao.
    let mut out = String::new();
    match char::decode_utf16(std::iter::once(unit)).next() {
        Some(Ok(ch)) => out.push(ch),
        _ => out.push('\u{FFFD}'),
    }
    alloc_str(&out)
}

#[unsafe(no_mangle)]
/// `str.charCodeAt(i)` — JS spec: retorna o UTF-16 code unit no
/// indice i (0-based, em code units). NaN (representado como -1 aqui)
/// quando fora de range.
pub extern "C" fn __RTS_FN_GL_STRING_CHAR_CODE_AT(recv: u64, idx: i64) -> i64 {
    let Some(s) = handle_to_str(recv) else {
        return -1;
    };
    if idx < 0 {
        return -1;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    units.get(idx as usize).map(|&u| u as i64).unwrap_or(-1)
}

/// `str.charCodeAt(idx)` — JS spec: retorna NaN (f64) fora de range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CHAR_CODE_AT_F64(recv: u64, idx: i64) -> f64 {
    let Some(s) = handle_to_str(recv) else {
        return f64::NAN;
    };
    if idx < 0 {
        return f64::NAN;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    match units.get(idx as usize) {
        Some(&u) => u as f64,
        None => f64::NAN,
    }
}

/// `str.codePointAt(i)` — JS spec: code point que comeca no code unit
/// `i`. Se `i` for high surrogate seguido de low, retorna o code point
/// composto (>= 0x10000). Senao retorna o code unit em `i`. Fora de
/// range retorna `undefined` (handle de string \"undefined\" — codegen
/// marca callsite como ambiguo).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CODE_POINT_AT(recv: u64, idx: i64) -> u64 {
    // Retorna uma WORD PolyValue (ret U64 na row): número inline (bits f64)
    // para um code point, `undefined` fora do range — a antiga ABI I64-número
    // devolvia o HANDLE da string "undefined" cru (imprimia 2814749767...).
    use rts_engine::heap::poly::POLY_UNDEFINED;
    let num = |cp: u32| (cp as f64).to_bits();
    let Some(s) = handle_to_str(recv) else {
        return POLY_UNDEFINED;
    };
    if idx < 0 {
        return POLY_UNDEFINED;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let i = idx as usize;
    let Some(&first) = units.get(i) else {
        return POLY_UNDEFINED;
    };
    // High surrogate seguido de low: combina.
    if (0xD800..=0xDBFF).contains(&first) {
        if let Some(&second) = units.get(i + 1) {
            if (0xDC00..=0xDFFF).contains(&second) {
                let high = (first as u32 - 0xD800) << 10;
                let low = second as u32 - 0xDC00;
                return num(high + low + 0x10000);
            }
        }
    }
    num(first as u32)
}

/// str.at(idx) — supports negative index (counts from end).
/// OOB returns the string "undefined" per JS spec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_AT(recv: u64, idx: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else {
        return alloc_str("undefined");
    };
    let count = s.chars().count() as i64;
    let i = if idx < 0 { count + idx } else { idx };
    if i < 0 || i >= count {
        return alloc_str("undefined");
    }
    match s.chars().nth(i as usize) {
        Some(ch) => {
            let mut buf = [0u8; 4];
            alloc_str(ch.encode_utf8(&mut buf))
        }
        None => alloc_str("undefined"),
    }
}

// ── Slicing methods ────────────────────────────────────────────────────────────

/// str.slice(start, end) — negative indices count from end.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SLICE(recv: u64, start: i64, end: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else {
        return alloc_str("");
    };
    let count = s.chars().count() as i64;
    let norm = |i: i64| -> usize {
        let n = if i < 0 { count + i } else { i };
        n.clamp(0, count) as usize
    };
    let si = norm(start);
    let ei = norm(end);
    if si >= ei {
        return alloc_str("");
    }
    let result: String = s.chars().skip(si).take(ei - si).collect();
    alloc_str(&result)
}

/// str.substring(start, end) — like slice but negatives clamp to 0, swaps if start>end.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SUBSTRING(recv: u64, start: i64, end: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else {
        return alloc_str("");
    };
    let count = s.chars().count() as i64;
    let clamp = |i: i64| i.clamp(0, count) as usize;
    let (si, ei) = {
        let a = clamp(start);
        let b = clamp(end);
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    if si >= ei {
        return alloc_str("");
    }
    let result: String = s.chars().skip(si).take(ei - si).collect();
    alloc_str(&result)
}

/// str.substr(start, length) — deprecated start+count form.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SUBSTR(recv: u64, start: i64, length: i64) -> u64 {
    let Some(s) = handle_to_str(recv) else {
        return alloc_str("");
    };
    let count = s.chars().count() as i64;
    let si = (if start < 0 { count + start } else { start }).clamp(0, count) as usize;
    let take = if length < 0 { 0 } else { length as usize };
    let result: String = s.chars().skip(si).take(take).collect();
    alloc_str(&result)
}

// ── Transform methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_UPPER_CASE(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(&s.to_uppercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_LOWER_CASE(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(&s.to_lowercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM_START(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim_start())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TRIM_END(recv: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    alloc_str(s.trim_end())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPEAT(recv: u64, n: i64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    if n <= 0 {
        return alloc_str("");
    }
    alloc_str(&s.repeat(n as usize))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE(recv: u64, from: u64, to: u64) -> u64 {
    match (handle_to_str(recv), handle_to_str(from), handle_to_str(to)) {
        (Some(s), Some(f), Some(t)) => alloc_str(&s.replacen(f, t, 1)),
        _ => recv,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE_ALL(recv: u64, from: u64, to: u64) -> u64 {
    match (handle_to_str(recv), handle_to_str(from), handle_to_str(to)) {
        (Some(s), Some(f), Some(t)) => alloc_str(&s.replace(f, t)),
        _ => recv,
    }
}

// _AUTO: dispatch (search regex/string) × (repl string/função) NO RUNTIME. O
// codegen chama UM símbolo; aqui inspecionamos os handles: `search` Entry::Regex
// → regex; `repl` Entry::String → string, senão é função (handle Function OU
// fn_ptr cru — REPLACE_REGEX_FN resolve ambos). String-search + função não tem
// variante runtime (= comportamento JS-aproximado antigo: trata repl como str).
#[inline]
fn h_is_regex(h: u64) -> bool {
    rts_engine::heap::handles::with_entry(h, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::Regex(_)))
    })
}
#[inline]
fn h_is_string(h: u64) -> bool {
    rts_engine::heap::handles::with_entry(h, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::String(_)))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE_AUTO(recv: u64, search: u64, repl: u64) -> u64 {
    if h_is_regex(search) {
        let (sp, sl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(recv), __RTS_FN_NS_GC_STRING_LEN(recv)) };
        if h_is_string(repl) {
            crate::string::search::__RTS_FN_NS_STRING_REPLACE_REGEX(sp, sl, search, repl)
        } else {
            crate::string::search::__RTS_FN_NS_STRING_REPLACE_REGEX_FN(sp, sl, search, repl)
        }
    } else {
        __RTS_FN_GL_STRING_REPLACE(recv, search, repl)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_REPLACE_ALL_AUTO(recv: u64, search: u64, repl: u64) -> u64 {
    if h_is_regex(search) {
        let (sp, sl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(recv), __RTS_FN_NS_GC_STRING_LEN(recv)) };
        if h_is_string(repl) {
            crate::string::search::__RTS_FN_NS_STRING_REPLACE_REGEX(sp, sl, search, repl)
        } else {
            crate::string::search::__RTS_FN_NS_STRING_REPLACE_REGEX_FN(sp, sl, search, repl)
        }
    } else {
        __RTS_FN_GL_STRING_REPLACE_ALL(recv, search, repl)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CONCAT(recv: u64, other: u64) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_CONCAT(recv, other) }
}

/// `str.concat(...args)` VARIÁDICO — o codegen empacota os args num Vec<i64> de
/// handles (pelo caminho genérico do `variadic` flag) e passa UM handle; o fold
/// vive aqui no runtime. recv + cada elemento, em ordem.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_CONCAT_VARIADIC(recv: u64, args_vec: u64) -> u64 {
    use rts_engine::heap::handles::{with_entry, Entry};
    let parts: Vec<i64> = with_entry(args_vec, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut acc = recv;
    for p in parts {
        acc = unsafe { __RTS_FN_NS_GC_STRING_CONCAT(acc, p as u64) };
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_START(recv: u64, target_len: i64, pad: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let fill = handle_to_str(pad).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || fill.is_empty() {
        return recv;
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = fill.chars().collect();
    let prefix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{prefix}{s}"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_PAD_END(recv: u64, target_len: i64, pad: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let fill = handle_to_str(pad).unwrap_or(" ");
    let count = s.chars().count() as i64;
    if count >= target_len || fill.is_empty() {
        return recv;
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = fill.chars().collect();
    let suffix: String = pad_chars.iter().cycle().take(needed).collect();
    alloc_str(&format!("{s}{suffix}"))
}

/// str.split(sep) — retorna Vec handle com string handles como elementos.
/// JS spec: split("") retorna chars individuais (sem leading/trailing empty).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT(recv: u64, sep: u64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let delim = handle_to_str(sep).unwrap_or("");
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    if delim.is_empty() {
        // JS spec: split("") = chars unicode (UTF-16 code units, mas v0
        // usa chars Unicode que e' mais correto).
        for c in s.chars() {
            let mut buf = [0u8; 4];
            let h = alloc_str(c.encode_utf8(&mut buf)) as i64;
            unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
        }
    } else {
        for part in s.split(delim) {
            let h = alloc_str(part) as i64;
            unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
        }
    }
    vec_h
}

/// (#208) str.split(sep, limit) — variant com truncamento.
/// Pega ate `limit` partes (cortando o resto). Limit < 0 ignora.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT_LIMIT(recv: u64, sep: u64, limit: i64) -> u64 {
    let s = handle_to_str(recv).unwrap_or("");
    let delim = handle_to_str(sep).unwrap_or("");
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    let take = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };
    if delim.is_empty() {
        for c in s.chars().take(take) {
            let mut buf = [0u8; 4];
            let h = alloc_str(c.encode_utf8(&mut buf)) as i64;
            unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
        }
    } else {
        for part in s.split(delim).take(take) {
            let h = alloc_str(part) as i64;
            unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
        }
    }
    vec_h
}

/// `str.split(sep, limit)` GENÉRICO — o codegen chama UM símbolo; o dispatch por
/// TIPO do separador (regex vs string) vive aqui no runtime (rts-shared), não no
/// codegen. Inspeciona o handle `sep`: `Entry::Regex` → split por regex; senão
/// → split por string. `limit` default i64::MAX = "sem limite" (take-all). É o
/// padrão `_AUTO` que substitui o `match` por tipo-de-arg hardcoded no codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT_AUTO(recv: u64, sep: u64, limit: i64) -> u64 {
    use rts_engine::heap::handles::{with_entry, Entry};
    let is_regex = with_entry(sep, |e| matches!(e, Some(Entry::Regex(_))));
    if is_regex {
        crate::string::search::__RTS_FN_GL_STRING_SPLIT_REGEX_LIMIT(recv, sep, limit)
    } else {
        __RTS_FN_GL_STRING_SPLIT_LIMIT(recv, sep, limit)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LOCALE_COMPARE(recv: u64, other: u64) -> i64 {
    use std::cmp::Ordering;
    match (handle_to_str(recv), handle_to_str(other)) {
        (Some(a), Some(b)) => {
            // (cross-runtime #156) JS locale order (ICU/CLDR): compare
            // case-insensitively primeiro; quando case-insensitive
            // iguais, lowercase vem ANTES de uppercase (oposto do
            // ASCII puro):
            //   "A".localeCompare("a") =  1   ("a" vem antes)
            //   "a".localeCompare("A") = -1
            // Em ASCII 'A' (0x41) < 'a' (0x61), entao precisamos
            // inverter o secondary tie-break (b.cmp(a) em vez de
            // a.cmp(b)) para bater com Bun/Node.
            let a_lo = a.to_lowercase();
            let b_lo = b.to_lowercase();
            match a_lo.cmp(&b_lo) {
                Ordering::Less => -1,
                Ordering::Greater => 1,
                Ordering::Equal => match b.cmp(a) {
                    Ordering::Less => -1,
                    Ordering::Equal => 0,
                    Ordering::Greater => 1,
                },
            }
        }
        _ => 0,
    }
}

// ── Identity / ES2024 ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_STRING(handle: u64) -> u64 {
    // Para handles que ja sao String, passthrough.
    // Para Symbol/Function, delega para TO_STRING_HANDLE que formata
    // \"Symbol(desc)\" / \"function name() { [native code] }\".
    // Para StringBox (#1023), unwrap para o String handle interno.
    // Para Map/Vec/etc, mantem passthrough (handle vai pra coercao
    // em concat/template via TPL_COERCE_AUTO).
    use rts_engine::heap::handles::{with_entry, Entry};
    enum Kind {
        NeedsFormat,
        Unwrap(u64),
        Passthrough,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Symbol { .. }) | Some(Entry::Function(_)) => Kind::NeedsFormat,
        Some(Entry::StringBox(h)) => Kind::Unwrap(*h),
        _ => Kind::Passthrough,
    });
    match kind {
        Kind::NeedsFormat => {
            crate::gc_surface::__RTS_FN_RT_TO_STRING_HANDLE(handle)
        }
        Kind::Unwrap(inner) => inner,
        Kind::Passthrough => handle,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_IS_WELL_FORMED(handle: u64) -> i64 {
    use rts_engine::heap::handles::{with_entry, Entry};
    with_entry(handle, |e| {
        let Some(Entry::String(b)) = e else {
            return 1_i64;
        };
        // Lone surrogates em WTF-8: 0xED seguido de 0xA0-0xBF indica surrogate.
        // Surrogate pairs validos nao aparecem como WTF-8 na representacao interna.
        let bytes = b.as_slice();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 3 <= bytes.len()
                && bytes[i] == 0xED
                && bytes[i + 1] >= 0xA0
                && bytes[i + 1] <= 0xBF
            {
                return 0_i64;
            }
            i += 1;
        }
        1_i64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_TO_WELL_FORMED(handle: u64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
    let replacement = [0xEFu8, 0xBF, 0xBD]; // U+FFFD
    let result = with_entry(handle, |e| {
        let Some(Entry::String(b)) = e else {
            return None;
        };
        let bytes = b.as_slice();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 3 <= bytes.len()
                && bytes[i] == 0xED
                && bytes[i + 1] >= 0xA0
                && bytes[i + 1] <= 0xBF
            {
                out.extend_from_slice(&replacement);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        Some(out)
    });
    match result {
        Some(bytes) => alloc_entry(Entry::String(bytes)),
        None => handle,
    }
}

/// `str.normalize(form?)` — Unicode normalization forms NFC/NFD/NFKC/NFKD.
/// Default form is NFC.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_NORMALIZE(recv: u64, form_h: u64) -> u64 {
    use unicode_normalization::UnicodeNormalization;
    let Some(s) = handle_to_str(recv) else {
        return recv;
    };
    let form = handle_to_str(form_h).unwrap_or("NFC");
    let normalized = match form {
        "NFD" => s.nfd().collect::<String>(),
        "NFKC" => s.nfkc().collect::<String>(),
        "NFKD" => s.nfkd().collect::<String>(),
        _ => s.nfc().collect::<String>(), // NFC is default
    };
    alloc_str(&normalized)
}

/// `str.length` — number of UTF-16 code units (JS spec).
/// Distinct from gc.string_len which returns UTF-8 byte count.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_LENGTH_UTF16(recv: u64) -> i64 {
    match handle_to_str(recv) {
        Some(s) => s.encode_utf16().count() as i64,
        None => 0,
    }
}
