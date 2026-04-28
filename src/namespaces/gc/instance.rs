//! Layout nativo de classes — passo 4 da #147.
//!
//! APIs raw para alocar/ler/escrever instancias com layout fixo, indexado
//! por `offset` em bytes. Layout calculado em compile-time pelo
//! `ClassLayout` (ver `codegen::lower::class_layout`). Aditivo nesta fase:
//! o codegen ainda nao consome estas APIs — sao a primitiva runtime que
//! viabiliza o switch dual-path em iteracoes futuras.
//!
//! Validacao defensiva: handle invalido ou offset fora do range retornam
//! 0 (loads) ou 0 (stores) sem trap.

use super::handles::{alloc_entry, free_handle, shard_for_handle, Entry, Instance};

const SENTINEL_ERR: i64 = 0;

/// Aloca uma nova instancia com `size` bytes zerados e tag de classe
/// `class_handle`. Retorna o handle, ou 0 em erro (size invalido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_NEW(size: i32, class_handle: u64) -> u64 {
    if size < 0 || size > (1 << 24) {
        return 0;
    }
    let bytes = vec![0u8; size as usize];
    let inst = Instance {
        class: class_handle,
        bytes,
    };
    alloc_entry(Entry::Instance(Box::new(inst)))
}

/// Retorna o class handle armazenado na instancia, ou 0 em handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_CLASS(h: u64) -> u64 {
    let table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get(h) else {
        return 0;
    };
    inst.class
}

/// Libera a instancia. Retorna 1 em sucesso, 0 se handle ja invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_FREE(h: u64) -> i64 {
    if free_handle(h) { 1 } else { 0 }
}

#[inline]
fn check_range(len: usize, offset: i32, slot: usize) -> Option<usize> {
    if offset < 0 {
        return None;
    }
    let off = offset as usize;
    if off.checked_add(slot)? > len {
        return None;
    }
    Some(off)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_LOAD_I64(h: u64, offset: i32) -> i64 {
    let table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get(h) else {
        return 0;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 8) else {
        return 0;
    };
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&inst.bytes[off..off + 8]);
    i64::from_le_bytes(buf)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_STORE_I64(h: u64, offset: i32, value: i64) -> i64 {
    let mut table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get_mut(h) else {
        return SENTINEL_ERR;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 8) else {
        return SENTINEL_ERR;
    };
    inst.bytes[off..off + 8].copy_from_slice(&value.to_le_bytes());
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_LOAD_I32(h: u64, offset: i32) -> i32 {
    let table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get(h) else {
        return 0;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 4) else {
        return 0;
    };
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&inst.bytes[off..off + 4]);
    i32::from_le_bytes(buf)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_STORE_I32(h: u64, offset: i32, value: i32) -> i64 {
    let mut table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get_mut(h) else {
        return SENTINEL_ERR;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 4) else {
        return SENTINEL_ERR;
    };
    inst.bytes[off..off + 4].copy_from_slice(&value.to_le_bytes());
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_LOAD_F64(h: u64, offset: i32) -> f64 {
    let table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get(h) else {
        return 0.0;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 8) else {
        return 0.0;
    };
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&inst.bytes[off..off + 8]);
    f64::from_le_bytes(buf)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_INSTANCE_STORE_F64(h: u64, offset: i32, value: f64) -> i64 {
    let mut table = shard_for_handle(h).lock().unwrap();
    let Some(Entry::Instance(inst)) = table.get_mut(h) else {
        return SENTINEL_ERR;
    };
    let Some(off) = check_range(inst.bytes.len(), offset, 8) else {
        return SENTINEL_ERR;
    };
    inst.bytes[off..off + 8].copy_from_slice(&value.to_le_bytes());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_class_load_store_i64() {
        let class_tag: u64 = 0xDEAD_BEEF;
        let h = __RTS_FN_NS_GC_INSTANCE_NEW(32, class_tag);
        assert_ne!(h, 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_CLASS(h), class_tag);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I64(h, 8, -1234), 1);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I64(h, 8), -1234);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_FREE(h), 1);
    }

    #[test]
    fn store_load_i32() {
        let h = __RTS_FN_NS_GC_INSTANCE_NEW(16, 1);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I32(h, 0, 0x01020304), 1);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I32(h, 0), 0x01020304);
        __RTS_FN_NS_GC_INSTANCE_FREE(h);
    }

    #[test]
    fn store_load_f64() {
        let h = __RTS_FN_NS_GC_INSTANCE_NEW(16, 1);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_F64(h, 0, std::f64::consts::PI), 1);
        let v = __RTS_FN_NS_GC_INSTANCE_LOAD_F64(h, 0);
        assert!((v - std::f64::consts::PI).abs() < 1e-12);
        __RTS_FN_NS_GC_INSTANCE_FREE(h);
    }

    #[test]
    fn invalid_handle_returns_zero() {
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_CLASS(0), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I64(0, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I64(0, 0, 1), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I32(0, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I32(0, 0, 1), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_F64(0, 0), 0.0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_F64(0, 0, 1.0), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_FREE(999_999), 0);
    }

    #[test]
    fn out_of_range_offset_returns_zero() {
        let h = __RTS_FN_NS_GC_INSTANCE_NEW(16, 1);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I64(h, 100), 0);
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I64(h, 100, 1), 0);
        // borda exata: 16 bytes, offset 9 + 8 > 16
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_STORE_I64(h, 9, 1), 0);
        // negativo
        assert_eq!(__RTS_FN_NS_GC_INSTANCE_LOAD_I64(h, -1), 0);
        __RTS_FN_NS_GC_INSTANCE_FREE(h);
    }
}
