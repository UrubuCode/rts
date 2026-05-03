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
}
