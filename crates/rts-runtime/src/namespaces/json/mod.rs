//! `json` namespace — JSON.parse / JSON.stringify primitives.
//!
//! `parse` retorna handle de um Entry::Json (`serde_json::Value` boxed).
//! `stringify` aceita um Entry::Json handle e devolve string handle.
//! Acesso a campos do JSON (path-based) e conversao pra tipos nativos
//! sao expostos via membros adicionais.
//!
//! Migrado ao modelo `#[rts_namespace]` (stage 2c,
//! `docs/specs/rts-core-engine.md`). As 5 fns extern nao-membro (PARSE5,
//! STRINGIFY_REPLACER_FN/KEYS/TYPED/PRETTY_STR) sao chamadas direto pelo
//! codegen por simbolo e ficam como free fns abaixo.

use serde_json::Value;

use rts_abi::ty::{Bool, Handle, F64, I64, U64};
use rts_macro::rts_namespace;

use super::gc::handles::{alloc_entry, free_handle, with_entry, Entry};

fn slice_from(ptr: u64, len: i64) -> Option<&'static [u8]> {
    if ptr == 0 || len < 0 {
        return None;
    }
    // SAFETY: o codegen entrega pares (ptr, len) provenientes de
    // gc.string_ptr ou de literais estaticos do binario, ambos validos
    // pela vida do programa do ponto de vista do callee.
    Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) })
}

/// (cross-runtime #228) Walk bottom-up (ECMA InternalizeJSONProperty): para
/// cada child, recursa primeiro com o holder atual como `this`, depois invoca
/// reviver(key, value) com `this`=holder.
fn apply_reviver(reviver_h: u64, holder: u64, key: &str) -> u64 {
    use crate::namespaces::collections::vec as v_ns;
    use crate::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED;
    // Pega o value (child) do holder.
    let value: u64 = with_entry(holder, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0) as u64,
        Some(Entry::Vec(arr)) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| arr.get(i).copied())
            .unwrap_or(0) as u64,
        _ => 0,
    });
    // Recursa em children do value (que vira novo holder).
    let kind = with_entry(value, |e| match e {
        Some(Entry::Map(m)) => Some((
            "map",
            m.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>(),
        )),
        Some(Entry::Vec(arr)) => Some((
            "vec",
            arr.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), *v))
                .collect::<Vec<_>>(),
        )),
        _ => None,
    });
    if let Some((k_type, entries)) = kind {
        for (k, _v) in entries {
            let new_v = apply_reviver(reviver_h, value, &k);
            // Atualiza slot.
            use crate::namespaces::gc::handles::with_entry_mut;
            with_entry_mut(value, |e| match e {
                Some(Entry::Map(m)) if k_type == "map" => {
                    if is_undefined_handle(new_v) {
                        m.shift_remove(&k);
                    } else {
                        m.insert(k.clone(), new_v as i64);
                    }
                }
                Some(Entry::Vec(arr)) if k_type == "vec" => {
                    if let Ok(idx) = k.parse::<usize>() {
                        if idx < arr.len() {
                            arr[idx] = if is_undefined_handle(new_v) {
                                i64::MIN + 2
                            } else {
                                new_v as i64
                            };
                        }
                    }
                }
                _ => {}
            });
        }
    }
    // Invoca reviver.call(holder, key, value).
    let key_h = alloc_entry(Entry::String(key.as_bytes().to_vec()));
    let args = alloc_entry(Entry::Vec(Box::new(vec![key_h as i64, value as i64])));
    let result = __RTS_FN_GL_FUNCTION_APPLY_TYPED(reviver_h, holder as i64, args);
    let _ = v_ns::__RTS_FN_NS_COLLECTIONS_VEC_GET;
    result as u64
}

fn is_undefined_handle(h: u64) -> bool {
    let v = h as i64;
    v == i64::MIN + 2 || v == i64::MIN + 4
}

/// JSON5.parse — extensao do JSON com comentarios, trailing commas,
/// keys nao-quoteadas, strings com aspas simples, hex literals, NaN/
/// Infinity, etc. Usa o crate `json5` que serializa para
/// `serde_json::Value`, entao a saida segue o mesmo bridge handle-table
/// do JSON estrito (sem semantica adicional fora parse-time).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_PARSE5(ptr: u64, len: i64) -> u64 {
    let Some(bytes) = slice_from(ptr, len) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    match json5::from_str::<Value>(text) {
        Ok(v) => json_value_to_handle(&v),
        Err(_) => 0,
    }
}

/// Converte recursivamente um `serde_json::Value` em handle nativo:
/// - Object -> Entry::Map (slots = handles ou i64 raw para scalars)
/// - Array -> Entry::Vec (idem)
/// - String -> Entry::String
/// - Number -> i64 raw (i64 direto ou f64.to_bits dentro do slot pai)
/// - Bool -> i64 raw 0/1
/// - Null -> i64 raw 0
///
/// Acesso JS-style direto (`obj.x`, `arr[0]`) le o slot raw, suficiente
/// pra `number` e `boolean`. APIs legacy (`json.array_get` etc) sintetizam
/// um wrapper Entry::Json on-demand para preservar o tipo escalar.
fn json_value_to_handle(v: &Value) -> u64 {
    match v {
        // (PR #1206) JS spec: JSON.parse("null") === null. RTS representa
        // null como sentinel `i64::MIN+3` para que `typeof` reporte
        // "object" e templates rendam "null". Antes retornava 0 que
        // colidia com handle invalido.
        Value::Null => (i64::MIN + 3) as u64,
        // Sentinels bool: MIN = false, MIN+1 = true. Caller (TPL_COERCE_AUTO,
        // typeof, etc) ja' decodifica.
        Value::Bool(b) => {
            if *b {
                (i64::MIN + 1) as u64
            } else {
                i64::MIN as u64
            }
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i as u64
            } else if let Some(f) = n.as_f64() {
                f.to_bits()
            } else {
                0
            }
        }
        Value::String(s) => alloc_entry(Entry::String(s.clone().into_bytes())),
        Value::Array(arr) => {
            let slots: Vec<i64> = arr.iter().map(|v| json_value_to_handle(v) as i64).collect();
            alloc_entry(Entry::Vec(Box::new(slots)))
        }
        Value::Object(map) => {
            let mut store: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
            for (k, val) in map {
                store.insert(k.clone(), json_value_to_handle(val) as i64);
            }
            alloc_entry(Entry::Map(Box::new(store)))
        }
    }
}

/// Inspeciona um handle JSON tratando todas as representacoes possiveis:
/// Entry::Json (escalar), Entry::Map (object), Entry::Vec (array),
/// Entry::String (string). Devolve `default` quando handle invalido ou tipo
/// desconhecido.
///
/// Importante: para Map/Vec apenas reconstroi a *forma* (vazio/nao-vazio)
/// dentro do lock. APIs estruturais (`array_len`, `array_get`, `object_get`,
/// `object_has`) NAO devem usar `with_json` — usam `with_entry` direto. Isso
/// evita deadlock por reentrancia: `handle_to_json_value` precisa pegar lock
/// de shards filhos, e fazer isso enquanto o lock do pai ainda esta segurado
/// trava se filho e pai estiverem no mesmo shard.
fn with_json<R>(handle: u64, default: R, f: impl FnOnce(&Value) -> R) -> R {
    // Snapshot do tipo (sem subgrafo) sob o lock; libera; depois invoca f.
    enum Snapshot {
        Json(Value),
        StringV(String),
        ArrayLen(usize),
        ObjectLen(usize),
        Missing,
    }
    let snap = with_entry(handle, |entry| match entry {
        Some(Entry::Json(v)) => Snapshot::Json(v.as_ref().clone()),
        Some(Entry::String(b)) => Snapshot::StringV(String::from_utf8_lossy(b).into_owned()),
        Some(Entry::Vec(slots)) => Snapshot::ArrayLen(slots.len()),
        Some(Entry::Map(map)) => Snapshot::ObjectLen(map.len()),
        _ => Snapshot::Missing,
    });
    match snap {
        Snapshot::Json(v) => f(&v),
        Snapshot::StringV(s) => f(&Value::String(s)),
        Snapshot::ArrayLen(n) => {
            // Reconstruir Array completo sem lock; usamos Null para slots
            // (suficiente para type_of/as_bool/array_len que so olham forma
            // e tamanho).
            let placeholder = Value::Array(vec![Value::Null; n]);
            f(&placeholder)
        }
        Snapshot::ObjectLen(n) => {
            let mut obj = serde_json::Map::with_capacity(n);
            for i in 0..n {
                obj.insert(format!("__placeholder_{i}"), Value::Null);
            }
            f(&Value::Object(obj))
        }
        Snapshot::Missing => default,
    }
}

/// (cross-runtime #743) ECMA-262 OrdinaryOwnPropertyKeys ordering: integer-indexed
/// keys (canonical u32 strings) ascendentes primeiro, depois demais em ordem
/// de insercao. Espelha o behavior de Bun/Node em Object.keys/JSON.stringify.
fn sort_ecma_keys(entries: Vec<(String, i64)>) -> Vec<(String, i64)> {
    fn parse_array_index(s: &str) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        if s.len() > 1 && s.starts_with('0') {
            return None;
        }
        if !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: u32 = s.parse().ok()?;
        if n == u32::MAX {
            return None;
        }
        Some(n)
    }
    let mut int_keys: Vec<(u32, String, i64)> = Vec::new();
    let mut str_keys: Vec<(String, i64)> = Vec::new();
    for (k, v) in entries {
        match parse_array_index(&k) {
            Some(n) => int_keys.push((n, k, v)),
            None => str_keys.push((k, v)),
        }
    }
    int_keys.sort_by_key(|(n, _, _)| *n);
    let mut out: Vec<(String, i64)> = Vec::with_capacity(int_keys.len() + str_keys.len());
    for (_, k, v) in int_keys {
        out.push((k, v));
    }
    out.extend(str_keys);
    out
}

// (cross-runtime #291) Setado quando stringify encontra um valor BigInt
// (representado por Entry::BigFixed como tag de BigInt em slot de objeto).
// JS spec: `JSON.stringify` de um BigInt lanca TypeError.
thread_local! {
    static JSON_BIGINT_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Serializa um handle de qualquer tipo conhecido (Map/Vec/String/Json) ou
/// devolve "" se nao reconhecido. Valores i64 dentro de Map/Vec sao tratados
/// como numero por padrao — nao ha como saber em runtime se i64 e' handle
/// string/map sem corromper interpretacao numerica.
fn stringify_any_inner(handle: u64) -> Option<String> {
    JSON_BIGINT_HIT.with(|c| c.set(false));
    let mut visited = std::collections::HashSet::new();
    let mut circular = false;
    let r = stringify_with_visited(handle, &mut visited, &mut circular);
    if JSON_BIGINT_HIT.with(|c| c.get()) {
        // (#291) BigInt nao e' serializavel -> TypeError (mesmo canal de pending
        // error que o caso circular; try/catch captura via __RTS_FN_RT_ERROR_GET).
        let err = alloc_entry(Entry::ErrorObj {
            message: "Do not know how to serialize a BigInt".to_owned(),
            name: "TypeError".to_owned(),
            cause: 0,
        });
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(err);
        return None;
    }
    if circular {
        // (#680/290) Sinaliza JSON circular como pending TypeError —
        // try/catch do TS captura via __RTS_FN_RT_ERROR_GET.
        let err = alloc_entry(Entry::ErrorObj {
            message: "Converting circular structure to JSON".to_owned(),
            name: "TypeError".to_owned(),
            cause: 0,
        });
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(err);
        return None;
    }
    r
}

fn stringify_with_visited(
    handle: u64,
    visited: &mut std::collections::HashSet<u64>,
    circular: &mut bool,
) -> Option<String> {
    use std::fmt::Write;
    if *circular {
        return None;
    }
    // (#98) Proxy: JSON.stringify itera as own keys enumeraveis via trap
    // `ownKeys` + `getOwnPropertyDescriptor`, e le cada valor via trap
    // `get` (mesmo trace de Bun/Node). Resolvemos pra um objeto plano
    // {key: value} e serializamos recursivamente.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        if !visited.insert(handle) {
            *circular = true;
            return None;
        }
        let keys_vec =
            crate::namespaces::globals::proxy::ops::dispatch_own_keys_enumerable(target, handler);
        let key_strs: Vec<String> = with_entry(keys_vec, |e| match e {
            Some(Entry::Vec(v)) => v
                .iter()
                .filter_map(|kh| {
                    with_entry(*kh as u64, |ke| match ke {
                        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                        _ => None,
                    })
                })
                .collect(),
            _ => Vec::new(),
        });
        let mut out = String::new();
        out.push('{');
        let mut first = true;
        for k in key_strs {
            let val = crate::namespaces::globals::proxy::ops::dispatch_get(target, handler, &k);
            if is_undefined_value(val) {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            out.push('"');
            for c in k.bytes() {
                match c {
                    b'"' => out.push_str("\\\""),
                    b'\\' => out.push_str("\\\\"),
                    _ => out.push(c as char),
                }
            }
            out.push_str("\":");
            out.push_str(&stringify_value_visited(val, visited, circular));
            if *circular {
                return None;
            }
        }
        out.push('}');
        visited.remove(&handle);
        return Some(out);
    }
    // Snapshot do Entry com lock minimo: copia bytes/entries para
    // estruturas owned, depois solta o Mutex do shard. Sem isso,
    // recursao em Map/Vec self-ref toma lock duplo no mesmo shard
    // e deadlocka (#680/290).
    enum Snap {
        Str(Vec<u8>),
        Map(Vec<(String, i64)>),
        Vec(Vec<i64>),
        Json(String),
        Date(i64),
        Other,
        None,
    }
    let snap = with_entry(handle, |e| match e {
        None => Snap::None,
        Some(Entry::String(b)) => Snap::Str(b.clone()),
        Some(Entry::Map(m)) => Snap::Map(m.iter().map(|(k, v)| (k.clone(), *v)).collect()),
        Some(Entry::Vec(v)) => Snap::Vec(v.as_ref().clone()),
        Some(Entry::Json(j)) => Snap::Json(serde_json::to_string(j.as_ref()).unwrap_or_default()),
        Some(Entry::DateMs(ms)) => Snap::Date(*ms),
        // (cross-runtime #291) Entry::BigFixed tagueia um BigInt em slot de
        // objeto. JS spec: BigInt nao e' serializavel -> sinaliza TypeError.
        Some(Entry::BigFixed(_)) => {
            JSON_BIGINT_HIT.with(|c| c.set(true));
            Snap::Other
        }
        Some(_) => Snap::Other,
    });
    let mut out = String::new();
    match snap {
        Snap::None | Snap::Other => None,
        Snap::Str(b) => {
            out.push('"');
            for c in b {
                match c {
                    b'"' => out.push_str("\\\""),
                    b'\\' => out.push_str("\\\\"),
                    b'\n' => out.push_str("\\n"),
                    b'\r' => out.push_str("\\r"),
                    b'\t' => out.push_str("\\t"),
                    0x00..=0x1f => {
                        let _ = write!(out, "\\u{:04x}", c);
                    }
                    _ => out.push(c as char),
                }
            }
            out.push('"');
            Some(out)
        }
        Snap::Map(entries) => {
            if !visited.insert(handle) {
                *circular = true;
                return None;
            }
            // (cross-runtime #743) ECMA-262 OrdinaryOwnPropertyKeys: integer-indexed
            // keys ("0", "1", ..., max u32) ascendentes; depois string keys em
            // ordem de insercao.
            let entries = sort_ecma_keys(entries);
            out.push('{');
            let mut first = true;
            for (k, val) in entries {
                if k == "__proto__" {
                    continue;
                }
                // (#110) Slots internos getter/setter — nao sao props publicas.
                if k.starts_with("__get_") || k.starts_with("__set_") {
                    continue;
                }
                // (#103) Symbol keys (encoded como `@@sym:<handle>` pelo
                // codegen) NAO sao serializadas em JSON.stringify — JS spec.
                if k.starts_with("@@sym:") {
                    continue;
                }
                // (cross-runtime #292) `__rts_class` eh o discriminator de
                // class instance (tag interno RTS). NAO deve aparecer em
                // JSON.stringify output — sem toJSON() dispatch real, ao menos
                // omitir o lixo. Tambem `__rts_desc_*` (Reflect property
                // descriptors) sao metadata interna.
                if k == "__rts_class" || k.starts_with("__rts_desc_") {
                    continue;
                }
                // (#680/50) JSON spec: omite props com valor undefined em obj.
                if is_undefined_value(val) {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push('"');
                for c in k.bytes() {
                    match c {
                        b'"' => out.push_str("\\\""),
                        b'\\' => out.push_str("\\\\"),
                        _ => out.push(c as char),
                    }
                }
                out.push_str("\":");
                out.push_str(&stringify_value_visited(val, visited, circular));
                if *circular {
                    return None;
                }
            }
            out.push('}');
            visited.remove(&handle);
            Some(out)
        }
        Snap::Vec(arr) => {
            if !visited.insert(handle) {
                *circular = true;
                return None;
            }
            out.push('[');
            for (i, val) in arr.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // (#680/50) JSON spec: undefined em array vira "null".
                if is_undefined_value(val) {
                    out.push_str("null");
                } else {
                    out.push_str(&stringify_value_visited(val, visited, circular));
                }
                if *circular {
                    return None;
                }
            }
            out.push(']');
            visited.remove(&handle);
            Some(out)
        }
        Snap::Json(s) => Some(s),
        // (#680/292) JSON.stringify(date) → ISO entre aspas (toJSON spec).
        Snap::Date(ms) => Some(date_iso_quoted(ms)),
    }
}

fn stringify_value_visited(
    v: i64,
    visited: &mut std::collections::HashSet<u64>,
    circular: &mut bool,
) -> String {
    // (PR #1211) NaN/+Infinity em JSON viram "null". Bits f64 conhecidos
    // que NAO colidem com sentinels do RTS (i64::MIN..MIN+4).
    // -Infinity bits = `0xFFF0_0000_0000_0000` = i64::MIN — colisao com
    // sentinel false. JS spec dize que -Infinity vira null em
    // JSON.stringify, e essa colisao significa que `false` em JSON
    // tambem viraria null por essa heuristica — incorreto. Por isso
    // tratamos -Infinity como excecao explicita: quando o valor i64::MIN
    // chega aqui via field declarado f64, fica null; quando vem de
    // sentinel bool, mantemos false. Distinguishing requer contexto
    // estatico; sem ele, o caso bool eh muito mais comum e mantemos.
    let bits = v as u64;
    if bits == f64::INFINITY.to_bits() || bits == f64::NAN.to_bits() {
        return "null".to_string();
    }
    // Outros bits NaN do f64 (mantissa nao-canonica) tambem viram null.
    let as_f64 = f64::from_bits(bits);
    if as_f64.is_nan() {
        return "null".to_string();
    }
    if v == i64::MIN {
        return "false".to_string();
    }
    if v == i64::MIN + 1 {
        return "true".to_string();
    }
    if v == i64::MIN + 2 || v == i64::MIN + 3 || v == i64::MIN + 4 {
        return "null".to_string();
    }
    let h = v as u64;
    if h > 0xFFFF_FFFF {
        if let Some(s) = stringify_with_visited(h, visited, circular) {
            return s;
        }
    }
    if *circular {
        return String::new();
    }
    v.to_string()
}

/// (#680/50) Detecta se valor representa `undefined` em RTS:
/// - sentinel i64::MIN+2 (undefined explicito)
/// - sentinel i64::MIN+4 (sparse hole, comporta como undefined)
/// - handle de Entry::String "undefined" (literal lowered)
fn is_undefined_value(v: i64) -> bool {
    if v == i64::MIN + 2 || v == i64::MIN + 4 {
        return true;
    }
    let h = v as u64;
    if h <= 0xFFFF_FFFF {
        return false;
    }
    with_entry(h, |e| match e {
        Some(Entry::String(b)) => b.as_slice() == b"undefined",
        _ => false,
    })
}

/// Helper: formata DateMs como ISO string entre aspas para JSON.
fn date_iso_quoted(ms: i64) -> String {
    let (y, mo, d, h, mi, s, mil) = super::date::date_unpack(ms);
    format!(
        "\"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z\"",
        y,
        mo + 1,
        d,
        h,
        mi,
        s,
        mil
    )
}

/// (cross-runtime #50) `JSON.stringify(value, replacer_fn)` — aplica
/// replacer recursivamente walk-top-down (key, value), depois stringify.
/// Cada chamada cria handle valor copia (snapshot) e substitui por
/// retorno do replacer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_REPLACER_FN(handle: u64, replacer_h: u64) -> u64 {
    let transformed = apply_stringify_replacer(replacer_h, "", handle);
    if is_undefined_handle(transformed) {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    __RTS_FN_NS_JSON_STRINGIFY(transformed)
}

/// Walk top-down: invoca replacer(key, value), depois recursa em children
/// do retorno (se Map/Vec). Cria Map/Vec NOVO em vez de mutar o original
/// (sem isso, segunda chamada JSON.stringify ve estado mutado).
fn apply_stringify_replacer(replacer_h: u64, key: &str, value: u64) -> u64 {
    use crate::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED;
    let key_h = alloc_entry(Entry::String(key.as_bytes().to_vec()));
    let args = alloc_entry(Entry::Vec(Box::new(vec![key_h as i64, value as i64])));
    let result = __RTS_FN_GL_FUNCTION_APPLY_TYPED(replacer_h, 0, args);
    let new_value = result as u64;
    if is_undefined_handle(new_value) {
        return new_value;
    }
    // Snapshot entries do new_value (para nao precisar holding lock).
    let kind = with_entry(new_value, |e| match e {
        Some(Entry::Map(m)) => Some((
            "map",
            m.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>(),
        )),
        Some(Entry::Vec(arr)) => Some((
            "vec",
            arr.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), *v))
                .collect::<Vec<_>>(),
        )),
        _ => None,
    });
    if let Some((k_type, entries)) = kind {
        // Cria NOVO container — preserva o original sem mutar.
        if k_type == "map" {
            let mut new_map = indexmap::IndexMap::<String, i64>::new();
            for (k, v) in entries {
                let recursed = apply_stringify_replacer(replacer_h, &k, v as u64);
                if !is_undefined_handle(recursed) {
                    new_map.insert(k, recursed as i64);
                }
            }
            alloc_entry(Entry::Map(Box::new(new_map)))
        } else {
            let mut new_arr: Vec<i64> = Vec::new();
            for (k, v) in entries {
                let recursed = apply_stringify_replacer(replacer_h, &k, v as u64);
                new_arr.push(if is_undefined_handle(recursed) {
                    i64::MIN + 2
                } else {
                    recursed as i64
                });
            }
            alloc_entry(Entry::Vec(Box::new(new_arr)))
        }
    } else {
        new_value
    }
}

/// `JSON.stringify(value, keys_array)` — replacer como array de keys filtra props.
/// `keys_array_h` deve ser handle de Entry::Vec contendo handles de Entry::String.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_KEYS(handle: u64, keys_array_h: u64) -> u64 {
    let keys: Vec<String> = with_entry(keys_array_h, |e| {
        let Some(Entry::Vec(arr)) = e else {
            return Vec::new();
        };
        arr.iter()
            .filter_map(|&slot| {
                let h = slot as u64;
                with_entry(h, |inner| {
                    if let Some(Entry::String(b)) = inner {
                        std::str::from_utf8(b).ok().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
            })
            .collect()
    });
    let s = stringify_with_keys(handle, &keys).unwrap_or_else(|| "null".to_string());
    alloc_entry(Entry::String(s.into_bytes()))
}

fn stringify_with_keys(handle: u64, keys: &[String]) -> Option<String> {
    use std::fmt::Write;
    with_entry(handle, |e| {
        let v = e?;
        let mut out = String::new();
        match v {
            Entry::String(b) => {
                out.push('"');
                for &c in b {
                    match c {
                        b'"' => out.push_str("\\\""),
                        b'\\' => out.push_str("\\\\"),
                        b'\n' => out.push_str("\\n"),
                        b'\r' => out.push_str("\\r"),
                        b'\t' => out.push_str("\\t"),
                        0x00..=0x1f => {
                            let _ = write!(out, "\\u{:04x}", c);
                        }
                        _ => out.push(c as char),
                    }
                }
                out.push('"');
                Some(out)
            }
            Entry::Map(m) => {
                out.push('{');
                let mut first = true;
                for k in keys {
                    if k == "__proto__" {
                        continue;
                    }
                    if k.starts_with("@@sym:") {
                        continue;
                    }
                    if let Some(val) = m.get(k) {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push('"');
                        for c in k.bytes() {
                            match c {
                                b'"' => out.push_str("\\\""),
                                b'\\' => out.push_str("\\\\"),
                                _ => out.push(c as char),
                            }
                        }
                        out.push_str("\":");
                        out.push_str(&stringify_value_with_keys(*val, keys));
                    }
                }
                out.push('}');
                Some(out)
            }
            Entry::Vec(arr) => {
                out.push('[');
                for (i, val) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&stringify_value_with_keys(*val, keys));
                }
                out.push(']');
                Some(out)
            }
            Entry::Json(j) => serde_json::to_string(j.as_ref()).ok(),
            _ => None,
        }
    })
}

fn stringify_value_with_keys(v: i64, keys: &[String]) -> String {
    if v == i64::MIN {
        return "false".to_string();
    }
    if v == i64::MIN + 1 {
        return "true".to_string();
    }
    if v == i64::MIN + 2 || v == i64::MIN + 3 || v == i64::MIN + 4 {
        return "null".to_string();
    }
    let h = v as u64;
    if h > 0xFFFF_FFFF {
        if let Some(s) = stringify_with_keys(h, keys) {
            return s;
        }
    }
    v.to_string()
}

/// `JSON.stringify` typed: o codegen passa um `kind` indicando o tipo
/// estatico do valor para preservar semantica JS (Boolean -> "true"/"false",
/// Number -> formato JS, String -> handle).
///
/// `kind`: 0=i64/handle (caminho legacy), 1=f64 bits, 2=bool, 3=null/undefined.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_TYPED(value: i64, kind: i32) -> u64 {
    // (PR #1206) JS spec: `JSON.stringify(undefined)` retorna `undefined`
    // (sentinel). Caller via TPL_COERCE_AUTO mapeia → "undefined" em
    // console.log. Detecta sentinel ANTES do match kind, porque mesmo
    // kind=0 pode chegar com sentinel quando o codegen nao tipo
    // estaticamente (var: any, member access, etc.).
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return (i64::MIN + 2) as u64;
    }
    let s = match kind {
        // Bool: 0/1 -> "false"/"true"
        2 => {
            if value != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        // null/undefined -> "null" (JS spec: JSON.stringify(null) === "null")
        3 => "null".to_string(),
        // f64 bits: numero JS-format. NaN/+-Infinity viram "null" (JS spec).
        1 => {
            let f = f64::from_bits(value as u64);
            if !f.is_finite() {
                "null".to_string()
            } else {
                crate::namespaces::gc::string_pool::format_js_number(f)
            }
        }
        // i64/handle: tenta como handle valido; senao formata como numero.
        _ => {
            let h = value as u64;
            if let Some(json) = stringify_any_inner(h) {
                json
            } else if h == 0 {
                "null".to_string()
            } else {
                value.to_string()
            }
        }
    };
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Pretty-printer recursivo para Map/Vec/String/Json com indent.
fn stringify_pretty_inner_str(handle: u64, indent: &str, depth: usize) -> Option<String> {
    use std::fmt::Write;
    let pad_outer = indent.repeat(depth);
    let pad_inner = indent.repeat(depth + 1);
    with_entry(handle, |e| {
        let v = e?;
        let mut out = String::new();
        match v {
            Entry::String(_) => stringify_any_inner(handle),
            Entry::Map(m) => {
                let entries: Vec<(&String, &i64)> = m
                    .iter()
                    .filter(|(k, _)| {
                        let s = k.as_str();
                        s != "__proto__"
                            && !s.starts_with("@@sym:")
                            && !s.starts_with("__get_")
                            && !s.starts_with("__set_")
                    })
                    .collect();
                if entries.is_empty() {
                    out.push_str("{}");
                    return Some(out);
                }
                out.push_str("{\n");
                for (i, (k, val)) in entries.iter().enumerate() {
                    out.push_str(&pad_inner);
                    out.push('"');
                    for c in k.bytes() {
                        match c {
                            b'"' => out.push_str("\\\""),
                            b'\\' => out.push_str("\\\\"),
                            _ => out.push(c as char),
                        }
                    }
                    out.push_str("\": ");
                    out.push_str(&stringify_pretty_value_i64_str(**val, indent, depth + 1));
                    if i + 1 < entries.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad_outer);
                out.push('}');
                Some(out)
            }
            Entry::Vec(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return Some(out);
                }
                out.push_str("[\n");
                for (i, val) in arr.iter().enumerate() {
                    out.push_str(&pad_inner);
                    out.push_str(&stringify_pretty_value_i64_str(*val, indent, depth + 1));
                    if i + 1 < arr.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad_outer);
                out.push(']');
                Some(out)
            }
            Entry::Json(j) => {
                let mut buf = Vec::with_capacity(64);
                let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
                let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
                serde::Serialize::serialize(j.as_ref(), &mut ser).ok()?;
                let _ = write!(out, "{}", String::from_utf8(buf).ok()?);
                Some(out)
            }
            _ => None,
        }
    })
}

fn stringify_pretty_value_i64_str(v: i64, indent: &str, depth: usize) -> String {
    let h = v as u64;
    if h > 0xFFFF_FFFF {
        if let Some(s) = stringify_pretty_inner_str(h, indent, depth) {
            return s;
        }
    }
    v.to_string()
}

/// `JSON.stringify(value, null, "<str>")` — indent eh string custom (ate
/// 10 chars conforme JS spec). String vazia desativa pretty.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_PRETTY_STR(handle: u64, indent_h: u64) -> u64 {
    let indent_str: String = match crate::namespaces::gc::string_pool::read_string_handle(indent_h)
    {
        Some(s) => {
            // JS spec: trunca em 10 caracteres.
            s.chars().take(10).collect()
        }
        None => String::new(),
    };
    if indent_str.is_empty() {
        return __RTS_FN_NS_JSON_STRINGIFY(handle);
    }
    if let Some(s) = stringify_pretty_inner_str(handle, &indent_str, 0) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    if handle == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    alloc_entry(Entry::String(handle.to_string().into_bytes()))
}

/// Quando um slot escalar (i64 raw) precisa virar handle JSON dirigivel
/// pelas APIs `as_i64`/`as_f64`/`type_of`/`as_string`, encapsulamos em
/// `Entry::Json(Number)`. Slots que ja sao handle de String/Vec/Map/Json
/// sao devolvidos tais quais.
fn promote_slot_to_json_handle(slot: u64) -> u64 {
    let already_handle = with_entry(slot, |e| {
        matches!(
            e,
            Some(Entry::Json(_))
                | Some(Entry::String(_))
                | Some(Entry::Vec(_))
                | Some(Entry::Map(_))
        )
    });
    if already_handle {
        slot
    } else {
        alloc_entry(Entry::Json(Box::new(Value::Number((slot as i64).into()))))
    }
}

/// JSON parsing and serialization (paridade com JSON.parse/stringify).
#[rts_namespace(json)]
impl JsonNs {
    /// Parses a JSON string into an opaque JSON value handle. Returns 0 on syntax error.
    #[rts_fn(ambiguous_ret)]
    pub fn parse(text: Str) -> U64 {
        match serde_json::from_str::<Value>(text) {
            Ok(v) => json_value_to_handle(&v),
            Err(_) => 0,
        }
    }

    /// JSON.parse com reviver(key, value) — transforma cada par durante o walk bottom-up.
    #[rts_fn(ambiguous_ret)]
    pub fn parse_reviver(text: Str, reviver: U64) -> U64 {
        let Ok(parsed) = serde_json::from_str::<Value>(text) else {
            return 0;
        };
        let root_handle = json_value_to_handle(&parsed);
        if reviver == 0 {
            return root_handle;
        }
        // JS spec InternalizeJSONProperty: cria holder raiz `{ "": root }` e chama
        // reviver com this=holder, key="", value=root.
        use indexmap::IndexMap;
        let mut root_holder: IndexMap<String, i64> = IndexMap::new();
        root_holder.insert(String::new(), root_handle as i64);
        let holder_h = alloc_entry(Entry::Map(Box::new(root_holder)));
        apply_reviver(reviver, holder_h, "")
    }

    /// Serializes a JSON value handle into its compact string form.
    #[rts_fn]
    pub fn stringify(value: U64) -> Handle {
        // (PR #1206) JS spec: `JSON.stringify(undefined)` retorna `undefined`
        // (nao string). RTS nao tem como expressar isso na ABI binaria
        // — retorna o sentinel undefined (i64::MIN+2) para sinalizar.
        // Callers que esperam string handle (ex: console.log direto) deveriam
        // dispatcher via TPL_COERCE_AUTO que mapeia sentinel → "undefined".
        let val_i64 = value as i64;
        if val_i64 == i64::MIN + 2 || val_i64 == i64::MIN + 4 {
            return (i64::MIN + 2) as u64;
        }
        if let Some(s) = stringify_any_inner(value) {
            return alloc_entry(Entry::String(s.into_bytes()));
        }
        // Fallback: handle 0 ou desconhecido vira "null".
        if value == 0 {
            return alloc_entry(Entry::String(b"null".to_vec()));
        }
        alloc_entry(Entry::String(value.to_string().into_bytes()))
    }

    /// Pretty-printed serialization with `indent` spaces (>= 0).
    #[rts_fn]
    pub fn stringify_pretty(value: U64, indent: I64) -> Handle {
        let indent = indent.max(0).min(16) as usize;
        if indent == 0 {
            return __RTS_FN_NS_JSON_STRINGIFY(value);
        }
        let indent_str = " ".repeat(indent);
        if let Some(s) = stringify_pretty_inner_str(value, &indent_str, 0) {
            return alloc_entry(Entry::String(s.into_bytes()));
        }
        if value == 0 {
            return alloc_entry(Entry::String(b"null".to_vec()));
        }
        alloc_entry(Entry::String(value.to_string().into_bytes()))
    }

    /// Releases the JSON value handle.
    #[rts_fn]
    pub fn free(handle: U64) {
        let _ = free_handle(handle);
    }

    /// Returns: 0 null, 1 bool, 2 number, 3 string, 4 array, 5 object, -1 invalid.
    #[rts_fn(pure)]
    pub fn type_of(value: U64) -> I64 {
        with_json(value, -1, |v| match v {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        })
    }

    /// Coerces JSON value to bool (true for non-zero/non-null/non-empty).
    #[rts_fn(pure)]
    pub fn as_bool(value: U64) -> Bool {
        with_json(value, 0, |v| match v {
            Value::Bool(b) => *b as i64,
            Value::Null => 0,
            Value::Number(n) => (n.as_f64().unwrap_or(0.0) != 0.0) as i64,
            Value::String(s) => (!s.is_empty()) as i64,
            Value::Array(a) => (!a.is_empty()) as i64,
            Value::Object(o) => (!o.is_empty()) as i64,
        })
    }

    /// Reads JSON number as i64 (truncates floats). 0 for invalid/non-number.
    #[rts_fn(pure)]
    pub fn as_i64(value: U64) -> I64 {
        with_json(value, 0, |v| match v {
            Value::Number(n) => n
                .as_i64()
                .or_else(|| n.as_f64().map(|f| f as i64))
                .unwrap_or(0),
            Value::Bool(b) => *b as i64,
            Value::String(s) => s.parse::<i64>().unwrap_or(0),
            _ => 0,
        })
    }

    /// Reads JSON number as f64. NaN for invalid/non-number.
    #[rts_fn(pure)]
    pub fn as_f64(value: U64) -> F64 {
        with_json(value, f64::NAN, |v| match v {
            Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
            Value::Bool(b) => *b as i64 as f64,
            Value::String(s) => s.parse::<f64>().unwrap_or(f64::NAN),
            _ => f64::NAN,
        })
    }

    /// Reads JSON string as a string handle. Empty handle (0) for non-string.
    #[rts_fn(pure)]
    pub fn as_string(value: U64) -> Handle {
        with_json(value, 0, |v| match v {
            Value::String(s) => alloc_entry(Entry::String(s.as_bytes().to_vec())),
            Value::Bool(b) => alloc_entry(Entry::String(b.to_string().into_bytes())),
            Value::Number(n) => alloc_entry(Entry::String(n.to_string().into_bytes())),
            Value::Null => alloc_entry(Entry::String(b"null".to_vec())),
            _ => 0,
        })
    }

    /// Number of elements when value is array; -1 otherwise.
    #[rts_fn(pure)]
    pub fn array_len(value: U64) -> I64 {
        with_entry(value, |entry| match entry {
            Some(Entry::Vec(slots)) => slots.len() as i64,
            Some(Entry::Json(v)) => match v.as_ref() {
                Value::Array(a) => a.len() as i64,
                _ => -1,
            },
            _ => -1,
        })
    }

    /// Returns a NEW handle to the element at `index`. 0 if out of range.
    #[rts_fn]
    pub fn array_get(value: U64, index: I64) -> U64 {
        if index < 0 {
            return 0;
        }
        let slot = with_entry(value, |entry| match entry {
            Some(Entry::Vec(slots)) => slots.get(index as usize).copied(),
            Some(Entry::Json(v)) => match v.as_ref() {
                Value::Array(a) => a
                    .get(index as usize)
                    .map(|child| alloc_entry(Entry::Json(Box::new(child.clone()))) as i64),
                _ => None,
            },
            _ => None,
        });
        match slot {
            Some(s) => promote_slot_to_json_handle(s as u64),
            None => 0,
        }
    }

    /// Returns a NEW handle to the property `key`. 0 if missing or non-object.
    #[rts_fn]
    pub fn object_get(value: U64, key: Str) -> U64 {
        let slot = with_entry(value, |entry| match entry {
            Some(Entry::Map(map)) => map.get(key).copied(),
            Some(Entry::Json(v)) => match v.as_ref() {
                Value::Object(o) => o
                    .get(key)
                    .map(|child| alloc_entry(Entry::Json(Box::new(child.clone()))) as i64),
                _ => None,
            },
            _ => None,
        });
        match slot {
            Some(s) => promote_slot_to_json_handle(s as u64),
            None => 0,
        }
    }

    /// True when value is an object containing `key`.
    #[rts_fn(pure)]
    pub fn object_has(value: U64, key: Str) -> Bool {
        with_entry(value, |entry| match entry {
            Some(Entry::Map(map)) => map.contains_key(key) as i64,
            Some(Entry::Json(v)) => match v.as_ref() {
                Value::Object(o) => o.contains_key(key) as i64,
                _ => 0,
            },
            _ => 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc_str(s: &str) -> (u64, u64, i64) {
        let h = alloc_entry(Entry::String(s.as_bytes().to_vec()));
        (h, s.as_ptr() as u64, s.len() as i64)
    }

    #[test]
    fn parse_object_roundtrip() {
        let src = r#"{"x":1,"y":"two","z":[true,null]}"#;
        let h = __RTS_FN_NS_JSON_PARSE(src.as_ptr(), src.len() as i64);
        assert_ne!(h, 0);
        assert_eq!(__RTS_FN_NS_JSON_TYPE_OF(h), 5); // object

        let str_h = __RTS_FN_NS_JSON_STRINGIFY(h);
        assert_ne!(str_h, 0);

        __RTS_FN_NS_JSON_FREE(h);
    }

    #[test]
    fn invalid_json_returns_zero() {
        let bad = "not json";
        let h = __RTS_FN_NS_JSON_PARSE(bad.as_ptr(), bad.len() as i64);
        assert_eq!(h, 0);
    }

    #[test]
    fn object_get_extracts_field() {
        let src = r#"{"name":"alice","age":30}"#;
        let root = __RTS_FN_NS_JSON_PARSE(src.as_ptr(), src.len() as i64);
        assert_ne!(root, 0);

        let key = "name";
        let name_h = __RTS_FN_NS_JSON_OBJECT_GET(root, key.as_ptr(), key.len() as i64);
        assert_ne!(name_h, 0);
        assert_eq!(__RTS_FN_NS_JSON_TYPE_OF(name_h), 3); // string

        let key = "age";
        let age_h = __RTS_FN_NS_JSON_OBJECT_GET(root, key.as_ptr(), key.len() as i64);
        assert_eq!(__RTS_FN_NS_JSON_AS_I64(age_h), 30);

        __RTS_FN_NS_JSON_FREE(root);
        __RTS_FN_NS_JSON_FREE(name_h);
        __RTS_FN_NS_JSON_FREE(age_h);
    }

    #[test]
    fn array_iteration() {
        let src = "[10, 20, 30]";
        let root = __RTS_FN_NS_JSON_PARSE(src.as_ptr(), src.len() as i64);
        assert_eq!(__RTS_FN_NS_JSON_ARRAY_LEN(root), 3);
        assert_eq!(
            __RTS_FN_NS_JSON_AS_I64(__RTS_FN_NS_JSON_ARRAY_GET(root, 0)),
            10
        );
        assert_eq!(
            __RTS_FN_NS_JSON_AS_I64(__RTS_FN_NS_JSON_ARRAY_GET(root, 2)),
            30
        );
        __RTS_FN_NS_JSON_FREE(root);
    }

    // Suppress unused warning for helper kept for symmetry with other tests.
    #[allow(dead_code)]
    fn _unused(s: &str) {
        let _ = alloc_str(s);
    }
}
