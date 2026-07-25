//! Spec + impl da classe primordial `Object`. Object é primordial: o motor
//! PODE nomeá-lo direto. Dispatch real da classe `Object` roda via o prelude
//! `.ts` embutido (`OBJECT_TS` em `lib.rs`), não pelo builder
//! `register_object_class_spec` (removido em DRAIN_MOTOR — nunca era chamado
//! por nenhum path de REGISTER/populate; era arquitetura paralela morta).
//!
//! (LAYERING FIX 2026-07-24) Os corpos `__RTS_FN_GL_OBJECT_*` MORAVAM em
//! `rts-shared/collections/map.rs` — Object é PRIMORDIAL, então o lugar certo
//! é aqui. `Entry::Map(IndexMap<String,i64>)` é definido em `rts-engine`
//! (heap-level, não algo de rts-shared), então essas fns operam direto sobre
//! o `Entry` do motor sem precisar de rts-shared. Os helpers `with_map`/
//! `with_map_mut`/`str_from_abi`/`parse_array_index`/`decode_symbol_key` são
//! réplicas locais e triviais dos equivalentes privados de
//! `rts-shared::collections::map` — duplicar essas poucas linhas puras evita
//! uma dependência rts-primitives → rts-shared (proibida pela direção do
//! grafo de crates: primitives só pode chamar engine).

use indexmap::IndexMap;

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

/// Reconhece "array index" no sentido do ECMA-262 (ver
/// `rts-shared::collections::map::parse_array_index` — mesma regra).
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

/// (#798) Decodifica `@@sym:<handle>` -> handle do Entry::Symbol.
fn decode_symbol_key(k: &str) -> Option<u64> {
    k.strip_prefix("@@sym:").and_then(|s| s.parse::<u64>().ok())
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn with_map<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&IndexMap<String, i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    })
}

fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexMap<String, i64>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    })
}

/// Set global de handles criados via `Object.create(null)` — preserva a
/// semântica "sem prototype" mesmo se o user setar `__proto__` depois (nesse
/// caso `__proto__` vira regular property).
fn null_proto_set() -> &'static std::sync::Mutex<std::collections::HashSet<u64>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u64>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

pub fn is_null_proto_handle(handle: u64) -> bool {
    null_proto_set().lock().unwrap().contains(&handle)
}

/// (#264 PR5) Cria novo Map vazio com `__proto__` = proto_handle.
/// Implementa `Object.create(proto)`. Quando `proto == 0`, equivale a
/// `Object.create(null)` — slot __proto__ setado explicit como 0 para
/// que MAP_GET_PROTO retorne null em vez do default Object.prototype.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_CREATE(proto: u64) -> u64 {
    let h = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    with_map_mut(h, (), |m| {
        m.insert("__proto__".to_string(), proto as i64);
    });
    // (#1080-format) Object.create(null) cria objeto sem prototype.
    // Marca para preservar a semantica mesmo se user setar __proto__
    // depois (em null-proto objects, __proto__ vira regular property).
    if proto == 0 {
        null_proto_set().lock().unwrap().insert(h);
    }
    h
}

/// (#162) `Object.create(proto, descriptors)` 2-arg variant — aplica
/// descriptors ao objeto. Cada entry de `descs` deve ser
/// `{ key: { value, writable?, enumerable?, configurable? } }`.
/// Para v0, extraimos so o `value` e fazemos MAP_SET. Tambem respeita
/// `enumerable: false` via mark_non_enumerable. writable/configurable
/// nao sao enforced ainda.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_APPLY_DESCRIPTORS(target: u64, descs: u64) {
    let pairs: Vec<(String, i64)> = with_entry(descs, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    for (key, desc_h_i) in pairs {
        let desc_h = desc_h_i as u64;
        // Extrai value + enumerable. Sentinel JS para false eh i64::MIN
        // (TPL coercion convention). enumerable defaulta a true quando
        // ausente em Object.defineProperty mas a false em Object.create
        // descriptors — aqui assumimos defineProperty-like (default true).
        let (value, is_non_enum): (i64, bool) = with_entry(desc_h, |e| match e {
            Some(Entry::Map(d)) => {
                let v = d.get("value").copied().unwrap_or(0);
                let enum_v = d.get("enumerable").copied();
                let non_enum = matches!(enum_v, Some(x) if x == i64::MIN);
                (v, non_enum)
            }
            _ => (desc_h_i, false),
        });
        with_map_mut(target, (), |m| {
            m.insert(key.clone(), value);
        });
        if is_non_enum {
            rts_engine::heap::descriptors::mark_non_enumerable(target, &key);
        }
    }
}

/// (#264 PR5) Verifica se `key` existe nas own props de `handle`
/// (sem seguir __proto__). Implementa \`obj.hasOwnProperty(key)\`.
/// Retorna 1 se own, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // Arrays (Vec): "length" eh own; chaves de indice (parsam como
    // uint32) sao own quando < length. Demais nao.
    let vec_result: Option<i64> = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            if key == "length" {
                Some(1)
            } else if let Some(n) = parse_array_index(key) {
                Some(if (n as usize) < v.len() { 1 } else { 0 })
            } else {
                Some(0)
            }
        }
        _ => None,
    });
    if let Some(r) = vec_result {
        return r;
    }
    with_map(handle, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

/// (cross-runtime #798) `Object.getOwnPropertySymbols(obj)` — retorna
/// Vec<i64> com handles de Symbol entries (keys `@@sym:<handle>`).
/// Ordem: insercao no IndexMap.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_SYMBOLS(handle: u64) -> u64 {
    let syms: Vec<i64> = with_map(handle, Vec::new(), |m| {
        m.keys()
            .filter_map(|k| decode_symbol_key(k).map(|h| h as i64))
            .collect()
    });
    alloc_entry(Entry::Vec(Box::new(syms)))
}

/// (cross-runtime #772) `obj.isPrototypeOf(other)` — walk `other.__proto__`
/// chain procurando `obj`. Retorna 1 se obj aparece na cadeia, 0 caso
/// contrario. Guard contra ciclos: profundidade maxima 64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF(obj: u64, other: u64) -> i64 {
    if obj == 0 || other == 0 {
        return 0;
    }
    let mut current = with_map(other, 0u64, |m| {
        m.get("__proto__").copied().unwrap_or(0) as u64
    });
    let mut depth = 0u32;
    while current != 0 && depth < 64 {
        if current == obj {
            return 1;
        }
        current = with_map(current, 0u64, |m| {
            m.get("__proto__").copied().unwrap_or(0) as u64
        });
        depth += 1;
    }
    0
}

/// (cross-runtime #788) `obj.propertyIsEnumerable(key)` — true se own
/// property + enumerable. Inherited (via __proto__) ja' retorna false
/// porque so' checa own. Para Vec: indices = enumerable, "length" = false.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    let vec_result: Option<i64> = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            // "length" eh own mas non-enumerable em arrays.
            if key == "length" {
                Some(0)
            } else if let Some(n) = parse_array_index(key) {
                Some(if (n as usize) < v.len() { 1 } else { 0 })
            } else {
                Some(0)
            }
        }
        _ => None,
    });
    if let Some(r) = vec_result {
        return r;
    }
    // Map: own + enumerable. defineProperty com enumerable:false marca via
    // is_non_enumerable.
    let has_own = with_map(handle, false, |m| m.contains_key(key));
    if !has_own {
        return 0;
    }
    if rts_engine::heap::descriptors::is_non_enumerable(handle, key) {
        return 0;
    }
    1
}
