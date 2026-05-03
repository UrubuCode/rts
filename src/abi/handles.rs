//! Layout do encoding de handles `u64`, compartilhado entre `gc::HandleTable`
//! e o handle table per-thread do `ui` (#283).
//!
//! ```text
//! [63..48] generation (16 bits)
//! [47.. 5] per-shard table slot (43 bits)
//! [ 4.. 0] shard index (5 bits, log2(N_SHARDS))
//! ```
//!
//! Mudancas aqui invalidam handles ja existentes — qualquer serializacao
//! futura (debug dump, IPC) precisa versionar o layout.

use crate::namespaces::gc::handles::{Entry, alloc_entry, set_vec_entry, set_map_entry, with_entry};

/// Numero de shards lock-free do `gc::HandleTable`.
pub const HANDLE_N_SHARDS: usize = 32;

/// Bits ocupados pelo indice de shard nas posicoes baixas do handle.
/// Sempre `log2(HANDLE_N_SHARDS)`.
pub const HANDLE_SHARD_BITS: u32 = HANDLE_N_SHARDS.ilog2();

/// Posicao do campo `generation` (16 bits) no handle `u64`.
pub const HANDLE_GEN_SHIFT: u32 = 48;

/// Mascara dos bits de slot+shard (tudo abaixo de `HANDLE_GEN_SHIFT`).
pub const HANDLE_SLOT_MASK: u64 = (1u64 << HANDLE_GEN_SHIFT) - 1;

/// Mascara do campo de shard (low bits).
pub const HANDLE_SHARD_MASK: u64 = (HANDLE_N_SHARDS as u64) - 1;

/// Wrapper para alocar um handle de string.
#[inline]
pub fn alloc_str_handle(bytes: &[u8]) -> u64 {
    alloc_entry(Entry::String(bytes.to_vec()))
}

/// Wrapper para alocar um handle de Vec<i64>.
#[inline]
pub fn alloc_vec_handle(vec: &Vec<i64>) -> u64 {
    alloc_entry(Entry::Vec(Box::new(vec.clone())))
}

/// Wrapper para alocar um handle de Map (IndexMap<String, i64>).
#[inline]
pub fn alloc_map_handle(map: &indexmap::IndexMap<String, i64>) -> u64 {
    alloc_entry(Entry::Map(Box::new(map.clone())))
}

/// Tenta obter um Vec<i64> a partir de um handle.
#[inline]
pub fn vec_from_handle(handle: u64) -> Option<std::vec::Vec<i64>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    })
}

/// Modifica um Vec<i64> a partir de um handle usando uma funcao de fechamento.
/// A funcao recebe uma copia do Vec, modifica e salva de volta atomicamente.
#[inline]
pub fn modify_vec_handle<F>(handle: u64, f: F) -> bool
where
    F: FnOnce(&mut Vec<i64>),
{
    if let Some(mut vec) = vec_from_handle(handle) {
        f(&mut vec);
        set_vec_entry(handle, vec);
        true
    } else {
        false
    }
}

/// Tenta obter um Map (IndexMap) a partir de um handle.
#[inline]
pub fn map_from_handle(handle: u64) -> Option<indexmap::IndexMap<String, i64>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => Some(m.as_ref().clone()),
        _ => None,
    })
}

/// Modifica um Map (IndexMap) a partir de um handle usando uma funcao de fechamento.
/// A funcao recebe uma copia do Map, modifica e salva de volta atomicamente.
#[inline]
pub fn modify_map_handle<F>(handle: u64, f: F) -> bool
where
    F: FnOnce(&mut indexmap::IndexMap<String, i64>),
{
    if let Some(mut map) = map_from_handle(handle) {
        f(&mut map);
        set_map_entry(handle, map);
        true
    } else {
        false
    }
}

/// StrHandle — wrapper type para handles de string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct StrHandle(pub u64);

impl StrHandle {
    #[inline]
    pub fn new(handle: u64) -> Self {
        StrHandle(handle)
    }
    
    #[inline]
    pub fn into_inner(self) -> u64 {
        self.0
    }
    
    /// Obtem os bytes da string.
    #[inline]
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        with_entry(self.0, |entry| match entry {
            Some(Entry::String(b)) => Some(b.clone()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_bits_matches_n_shards() {
        assert_eq!(1usize << HANDLE_SHARD_BITS, HANDLE_N_SHARDS);
    }

    #[test]
    fn slot_mask_excludes_generation() {
        assert_eq!(HANDLE_SLOT_MASK >> HANDLE_GEN_SHIFT, 0);
        assert_eq!(HANDLE_SLOT_MASK | (0xFFFFu64 << HANDLE_GEN_SHIFT), u64::MAX);
    }

    #[test]
    fn shard_mask_fits_in_low_bits() {
        assert_eq!(HANDLE_SHARD_MASK, (1u64 << HANDLE_SHARD_BITS) - 1);
    }
    
    #[test]
    fn alloc_str_roundtrip() {
        let h = alloc_str_handle(b"hello");
        let bytes = with_entry(h, |e| match e {
            Some(Entry::String(b)) => Some(b.clone()),
            _ => None,
        });
        assert_eq!(bytes, Some(b"hello".to_vec()));
    }
    
    #[test]
    fn alloc_vec_roundtrip() {
        let v = vec![1i64, 2, 3];
        let h = alloc_vec_handle(&v);
        let retrieved = vec_from_handle(h);
        assert_eq!(retrieved, Some(v));
    }
}
