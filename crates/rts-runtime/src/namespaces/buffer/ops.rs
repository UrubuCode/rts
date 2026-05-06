//! Operacoes sobre buffers binarios.
//!
//! Alocacao via HandleTable (`gc::handles`). Cada buffer vira um
//! `Entry::Buffer(Vec<u8>)` — `alloc` retorna handle u64, `free`
//! libera slot (mark + bump generation).
//!
//! Reads/writes sao `i64` offset + leitura little-endian das
//! representacoes nativas. Out-of-bounds retorna 0 (para reads) ou
//! vira no-op (para writes) — sem panics no boundary C.

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};

// Para o runtime staticlib, `super::super::gc` resolve para
// `crate::gc` (sem `namespaces`). Para o crate rts principal, resolve
// para `crate::namespaces::gc`. Ambos expoem `handles::{table, Entry}`.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn with_buffer_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Vec<u8>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Buffer(buf)) => f(buf),
        _ => default,
    })
}

fn with_buffer<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<u8>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Buffer(buf)) => f(buf),
        _ => default,
    })
}

/// Aloca um buffer de `size` bytes, preenchido com zeros.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_ALLOC(size: i64) -> u64 {
    if size < 0 {
        return 0;
    }
    let buf = vec![0u8; size as usize];
    alloc_entry(Entry::Buffer(buf))
}

/// Alias explicito para alloc zeroed — no Rust Vec::new ja zera.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_ALLOC_ZEROED(size: i64) -> u64 {
    __RTS_FN_NS_BUFFER_ALLOC(size)
}

/// Libera o handle. Chamadas repetidas sao no-op silencioso.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_FREE(handle: u64) {
    free_handle(handle);
}

/// Tamanho do buffer em bytes, ou -1 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_LEN(handle: u64) -> i64 {
    with_buffer(handle, -1, |b| b.len() as i64)
}

/// Ponteiro bruto para o inicio do buffer. Uso inseguro — serve para
/// interop com APIs que esperam `*const u8` (ex: io.stdout_write).
/// Retorna 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_PTR(handle: u64) -> u64 {
    with_buffer(handle, 0, |b| b.as_ptr() as u64)
}

// ── Reads ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_U8(handle: u64, offset: i64) -> i32 {
    with_buffer(handle, 0, |b| {
        if offset < 0 {
            return 0;
        }
        b.get(offset as usize).copied().unwrap_or(0) as i32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_I32(handle: u64, offset: i64) -> i32 {
    with_buffer(handle, 0, |b| {
        if offset < 0 {
            return 0;
        }
        let start = offset as usize;
        let end = start.saturating_add(4);
        if end > b.len() {
            return 0;
        }
        let bytes: [u8; 4] = b[start..end].try_into().unwrap();
        i32::from_le_bytes(bytes)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_I64(handle: u64, offset: i64) -> i64 {
    with_buffer(handle, 0, |b| {
        if offset < 0 {
            return 0;
        }
        let start = offset as usize;
        let end = start.saturating_add(8);
        if end > b.len() {
            return 0;
        }
        let bytes: [u8; 8] = b[start..end].try_into().unwrap();
        i64::from_le_bytes(bytes)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_F64(handle: u64, offset: i64) -> f64 {
    with_buffer(handle, f64::NAN, |b| {
        if offset < 0 {
            return f64::NAN;
        }
        let start = offset as usize;
        let end = start.saturating_add(8);
        if end > b.len() {
            return f64::NAN;
        }
        let bytes: [u8; 8] = b[start..end].try_into().unwrap();
        f64::from_le_bytes(bytes)
    })
}

// ── Writes ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_U8(handle: u64, offset: i64, val: i32) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        if let Some(slot) = b.get_mut(offset as usize) {
            *slot = val as u8;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_I32(handle: u64, offset: i64, val: i32) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        let start = offset as usize;
        let end = start.saturating_add(4);
        if end > b.len() {
            return;
        }
        b[start..end].copy_from_slice(&val.to_le_bytes());
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_I64(handle: u64, offset: i64, val: i64) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        let start = offset as usize;
        let end = start.saturating_add(8);
        if end > b.len() {
            return;
        }
        b[start..end].copy_from_slice(&val.to_le_bytes());
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_F64(handle: u64, offset: i64, val: f64) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        let start = offset as usize;
        let end = start.saturating_add(8);
        if end > b.len() {
            return;
        }
        b[start..end].copy_from_slice(&val.to_le_bytes());
    });
}

// ── Bulk ops ─────────────────────────────────────────────────────────

/// Copia `len` bytes de `src[src_off..]` para `dst[dst_off..]`.
/// Faz clone no meio para evitar borrow conflitante (src e dst podem
/// ser o mesmo handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_COPY(
    dst: u64,
    dst_off: i64,
    src: u64,
    src_off: i64,
    len: i64,
) {
    if len <= 0 || dst_off < 0 || src_off < 0 {
        return;
    }
    let data = with_buffer(src, Vec::new(), |b| {
        let start = src_off as usize;
        let end = start.saturating_add(len as usize);
        if end > b.len() {
            Vec::new()
        } else {
            b[start..end].to_vec()
        }
    });
    if data.is_empty() {
        return;
    }
    with_buffer_mut(dst, (), |b| {
        let start = dst_off as usize;
        let end = start.saturating_add(data.len());
        if end > b.len() {
            return;
        }
        b[start..end].copy_from_slice(&data);
    });
}

/// Preenche `len` bytes a partir do inicio com `byte`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_FILL(handle: u64, byte: i32, len: i64) {
    if len <= 0 {
        return;
    }
    with_buffer_mut(handle, (), |b| {
        let end = (len as usize).min(b.len());
        for slot in &mut b[..end] {
            *slot = byte as u8;
        }
    });
}

/// Compara conteudo byte-a-byte de dois buffers. Retorna 1 se iguais,
/// 0 se diferentes (ou algum handle invalido). Equivalente a
/// `Buffer.prototype.equals` em node:buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_EQUALS(a: u64, b: u64) -> i32 {
    if a == b {
        // Mesmo handle (ou ambos zero) — trivialmente iguais se vivos.
        return with_buffer(a, 0, |_| 1);
    }
    // Clonamos pra evitar segurar dois locks simultaneos (potencial
    // deadlock com shards distintos).
    let bytes_a = with_buffer(a, Vec::new(), |buf| buf.clone());
    let bytes_b = with_buffer(b, Vec::new(), |buf| buf.clone());
    if bytes_a == bytes_b { 1 } else { 0 }
}

/// Procura o primeiro byte com valor `byte` a partir de `from`.
/// Retorna o offset (>= 0) ou -1 se nao encontrado / handle invalido.
/// Equivalente a `Buffer.prototype.indexOf` (variante byte).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_INDEX_OF(handle: u64, byte: i32, from: i64) -> i64 {
    let target = byte as u8;
    with_buffer(handle, -1, |buf| {
        let start = if from < 0 { 0 } else { (from as usize).min(buf.len()) };
        match buf[start..].iter().position(|&b| b == target) {
            Some(i) => (start + i) as i64,
            None => -1,
        }
    })
}

/// Converte o buffer (assumido como UTF-8) para um string handle do
/// `gc::string_pool`. Conteudo invalido volta como string vazia.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_TO_STRING(handle: u64) -> u64 {
    // Clona os bytes antes de chamar STRING_NEW: o callback de
    // with_buffer segura o lock do HandleTable, e STRING_NEW tambem
    // tenta adquirir o mesmo lock — chamar dentro do callback gera
    // deadlock.
    let bytes = with_buffer(handle, Vec::new(), |b| b.clone());
    let text = std::str::from_utf8(&bytes).unwrap_or("");
    unsafe { __RTS_FN_NS_GC_STRING_NEW(text.as_ptr(), text.len() as i64) }
}
