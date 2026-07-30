//! Helpers de chave/acesso sobre `Entry::Map` — a FONTE ÚNICA desses cinco
//! helpers puros.
//!
//! Eles nasceram privados em `rts-shared::collections::map` e foram REPLICADOS
//! byte-a-byte em `rts-primitives::object` quando os corpos
//! `__RTS_FN_GL_OBJECT_*` desceram pra rts-primitives (LAYERING FIX 2026-07-24):
//! `primitives → shared` é um CICLO (shared já depende de primitives), então a
//! cópia era a única saída *naquele lugar*. O lugar certo é aqui: `Entry::Map`
//! é um tipo do heap do motor, e ambas as crates já dependem de `rts-engine`.
//! Uma definição, dois consumidores, zero aresta nova.

use indexmap::IndexMap;

use crate::heap::handles::{Entry, with_entry, with_entry_mut};

/// Reconhece "array index" no sentido do ECMA-262: string que representa
/// um u32 canônico (sem leading zeros exceto "0"; máximo 2^32 - 2).
/// Retorna o valor numérico para ordenação. Strings como "01", "+1", "1.0",
/// " 1" não são consideradas índices.
pub fn parse_array_index(s: &str) -> Option<u32> {
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

/// (#798) Keys com prefixo `@@sym:` sao a repr canonica de Symbol entries
/// (escrita pelo codegen via OBJ_SET/MAP_SET_KH). NAO sao expostas em
/// Object.keys/getOwnPropertyNames/values/entries — JS spec separa
/// string-keyed (Object.keys) de symbol-keyed (getOwnPropertySymbols).
pub fn is_symbol_key(k: &str) -> bool {
    k.starts_with("@@sym:")
}

/// (#798) Decodifica `@@sym:<handle>` -> handle do Entry::Symbol.
pub fn decode_symbol_key(k: &str) -> Option<u64> {
    k.strip_prefix("@@sym:").and_then(|s| s.parse::<u64>().ok())
}

/// Materializa um `&str` a partir do par `(ptr, len)` da ABI `StrPtr`.
///
/// # Safety
/// Contrato do chamador: `ptr`/`len` descrevem um buffer UTF-8 vivo pelo tempo
/// do empréstimo (é o contrato da ABI `StrPtr`, ver `rts_abi::AbiType::StrPtr`).
pub fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

pub fn with_map<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&IndexMap<String, i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    })
}

pub fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexMap<String, i64>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    })
}
