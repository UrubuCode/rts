//! `hash` namespace — non-cryptographic hashing via `std::hash::DefaultHasher`
//! (SipHash-1-3).
//!
//! Uso principal: chaves de HashMap proprio, deduplicacao, checksums rapidos.
//! **Nao use para seguranca** — SipHash resiste a HashDoS mas nao e pre-image
//! resistente. Para SHA/BLAKE veja namespace `crypto`.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rts_abi::ty::I64;
use rts_macro::rts_namespace;

fn hash_slice(bytes: &[u8]) -> i64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish() as i64
}

/// Non-cryptographic hashing via std::hash::DefaultHasher (SipHash-1-3).
#[rts_namespace(hash)]
impl HashNs {
    /// SipHash de uma string UTF-8.
    #[rts_fn(pure)]
    pub fn hash_str(s: Str) -> I64 {
        hash_slice(s.as_bytes())
    }

    /// SipHash de uma regiao de memoria (ptr + len). Use com buffer.ptr(handle).
    #[rts_fn(pure)]
    pub fn hash_bytes(ptr: I64, len: I64) -> I64 {
        if ptr == 0 || len < 0 {
            return 0;
        }
        // SAFETY: caller passou um ponteiro valido vindo de buffer/gc.
        let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        hash_slice(slice)
    }

    /// SipHash de um inteiro de 64 bits.
    #[rts_fn(pure)]
    pub fn hash_i64(value: I64) -> I64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish() as i64
    }

    /// Combina dois hashes preservando entropia (estilo boost::hash_combine).
    #[rts_fn(pure)]
    pub fn hash_combine(h1: I64, h2: I64) -> I64 {
        // Constante = golden ratio truncada. Aritmetica bitwise em u64
        // pra preservar os bits altos; shift << 6 em i64 sinalizado seria
        // ambiguo em valores negativos.
        let (a, b) = (h1 as u64, h2 as u64);
        let combined = a ^ b
            .wrapping_add(0x517c_c1b7_2722_0a95)
            .wrapping_add(a << 6)
            .wrapping_add(a >> 2);
        combined as i64
    }
}
