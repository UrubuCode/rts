//! `buffer` namespace — Vec<u8> via HandleTable para dados binarios.
//!
//! Alocacao via HandleTable (`gc::handles`). Cada buffer vira um
//! `Entry::Buffer(Vec<u8>)` — `alloc` retorna handle u64, `free` libera slot.
//! Reads/writes sao `i64` offset + leitura little-endian das representacoes
//! nativas. Out-of-bounds retorna 0 (reads) ou vira no-op (writes).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime). As
//! fns `__RTS_FN_GL_*` (DataView, ArrayBuffer, TypedArray view, Atomics) sao
//! metodos de classes globais, nao membros do namespace — ficam como free fns
//! abaixo, chamadas direto pelo codegen por simbolo.

use rts_engine::abi::ty::{Bool, Handle, F64, I32, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::gc::handles::{alloc_entry, free_handle, with_entry, with_entry_mut, Entry};

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

// ── Membros do namespace `buffer` (extern "C" + builder, sem macro) ──────────

/// Allocates a zero-initialised byte buffer of `size` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_ALLOC(size: I64) -> Handle {
    if size < 0 {
        return 0;
    }
    let buf = vec![0u8; size as usize];
    alloc_entry(Entry::Buffer(buf))
}

/// Alias for alloc — Rust Vec::new already zeroes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_ALLOC_ZEROED(size: I64) -> Handle {
    __RTS_FN_NS_BUFFER_ALLOC(size)
}

/// Releases the buffer handle. Subsequent reads/writes are no-ops.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_FREE(handle: U64) {
    free_handle(handle);
}

/// Buffer length in bytes, or -1 if the handle is invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_LEN(handle: U64) -> I64 {
    with_buffer(handle, -1, |b| b.len() as i64)
}

/// Raw pointer to the buffer start, or 0 when invalid. Unsafe — callers must not outlive the handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_PTR(handle: U64) -> U64 {
    with_buffer(handle, 0, |b| b.as_ptr() as u64)
}

/// Reads the byte at `offset`. Returns 0 out of bounds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_U8(handle: U64, offset: I64) -> I32 {
    with_buffer(handle, 0, |b| {
        if offset < 0 {
            return 0;
        }
        b.get(offset as usize).copied().unwrap_or(0) as i32
    })
}

/// Reads a little-endian i32 at `offset`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_I32(handle: U64, offset: I64) -> I32 {
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

/// Reads a little-endian i64 at `offset`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_I64(handle: U64, offset: I64) -> I64 {
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

/// Reads a little-endian f64 at `offset`. NaN out of bounds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_F64(handle: U64, offset: I64) -> F64 {
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

/// Reads a little-endian f32 at `offset`, widened to number. NaN out of bounds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_READ_F32(handle: U64, offset: I64) -> F64 {
    with_buffer(handle, f64::NAN, |b| {
        if offset < 0 {
            return f64::NAN;
        }
        let start = offset as usize;
        let end = start.saturating_add(4);
        if end > b.len() {
            return f64::NAN;
        }
        let bytes: [u8; 4] = b[start..end].try_into().unwrap();
        f32::from_le_bytes(bytes) as f64
    })
}

/// Writes `val` as u8 at `offset`. No-op out of bounds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_U8(handle: U64, offset: I64, val: I32) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        if let Some(slot) = b.get_mut(offset as usize) {
            *slot = val as u8;
        }
    });
}

/// Writes a little-endian i32 at `offset`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_I32(handle: U64, offset: I64, val: I32) {
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

/// Writes a little-endian i64 at `offset`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_I64(handle: U64, offset: I64, val: I64) {
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

/// Writes a little-endian f64 at `offset`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_F64(handle: U64, offset: I64, val: F64) {
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

/// Writes `val` as a little-endian f32 at `offset` (narrowed from number). No-op out of bounds. Ideal for audio sample buffers.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_WRITE_F32(handle: U64, offset: I64, val: F64) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        let start = offset as usize;
        let end = start.saturating_add(4);
        if end > b.len() {
            return;
        }
        b[start..end].copy_from_slice(&(val as f32).to_le_bytes());
    });
}

/// Copies `len` bytes from src+srcOff to dst+dstOff. Safe with overlapping src/dst.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_COPY(dst: U64, dst_off: I64, src: U64, src_off: I64, len: I64) {
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

/// Fills the first `len` bytes with `byte`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_FILL(handle: U64, byte: I32, len: I64) {
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

/// Interprets buffer contents as UTF-8 and returns a string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_TO_STRING(handle: U64) -> Handle {
    // Clona os bytes antes de chamar STRING_NEW: o callback de
    // with_buffer segura o lock do HandleTable, e STRING_NEW tambem
    // tenta adquirir o mesmo lock — chamar dentro do callback gera
    // deadlock.
    let bytes = with_buffer(handle, Vec::new(), |b| b.clone());
    let text = std::str::from_utf8(&bytes).unwrap_or("");
    unsafe { __RTS_FN_NS_GC_STRING_NEW(text.as_ptr(), text.len() as i64) }
}

/// Byte-wise equality of two buffers.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_EQUALS(a: U64, b: U64) -> Bool {
    if a == b {
        // Mesmo handle (ou ambos zero) — trivialmente iguais se vivos.
        return with_buffer(a, 0, |_| 1);
    }
    // Clonamos pra evitar segurar dois locks simultaneos (potencial
    // deadlock com shards distintos).
    let bytes_a = with_buffer(a, Vec::new(), |buf| buf.clone());
    let bytes_b = with_buffer(b, Vec::new(), |buf| buf.clone());
    if bytes_a == bytes_b {
        1
    } else {
        0
    }
}

/// Index of first occurrence of byte starting at `from`. Returns -1 if not found.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BUFFER_INDEX_OF(handle: U64, byte: I32, from: I64) -> I64 {
    let target = byte as u8;
    with_buffer(handle, -1, |buf| {
        let start = if from < 0 {
            0
        } else {
            (from as usize).min(buf.len())
        };
        match buf[start..].iter().position(|&b| b == target) {
            Some(i) => (start + i) as i64,
            None => -1,
        }
    })
}

/// Função `buffer.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8, pure: bool) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a namespace `buffer` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("buffer")
        .doc("Binary buffers backed by Vec<u8> in the handle table.")
        .member(func(
            "alloc",
            "__RTS_FN_NS_BUFFER_ALLOC",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "alloc(size: number): number",
            "Allocates a zero-initialised byte buffer of `size` bytes.",
            __RTS_FN_NS_BUFFER_ALLOC as *const u8,
            false,
        ))
        .member(func(
            "alloc_zeroed",
            "__RTS_FN_NS_BUFFER_ALLOC_ZEROED",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "alloc_zeroed(size: number): number",
            "Alias for alloc — Rust Vec::new already zeroes.",
            __RTS_FN_NS_BUFFER_ALLOC_ZEROED as *const u8,
            false,
        ))
        .member(func(
            "free",
            "__RTS_FN_NS_BUFFER_FREE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "free(handle: number): void",
            "Releases the buffer handle. Subsequent reads/writes are no-ops.",
            __RTS_FN_NS_BUFFER_FREE as *const u8,
            false,
        ))
        .member(func(
            "len",
            "__RTS_FN_NS_BUFFER_LEN",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "len(handle: number): number",
            "Buffer length in bytes, or -1 if the handle is invalid.",
            __RTS_FN_NS_BUFFER_LEN as *const u8,
            false,
        ))
        .member(func(
            "ptr",
            "__RTS_FN_NS_BUFFER_PTR",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "ptr(handle: number): number",
            "Raw pointer to the buffer start, or 0 when invalid. Unsafe — callers must not outlive the handle.",
            __RTS_FN_NS_BUFFER_PTR as *const u8,
            false,
        ))
        .member(func(
            "read_u8",
            "__RTS_FN_NS_BUFFER_READ_U8",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I32),
            "read_u8(handle: number, offset: number): number",
            "Reads the byte at `offset`. Returns 0 out of bounds.",
            __RTS_FN_NS_BUFFER_READ_U8 as *const u8,
            false,
        ))
        .member(func(
            "read_i32",
            "__RTS_FN_NS_BUFFER_READ_I32",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I32),
            "read_i32(handle: number, offset: number): number",
            "Reads a little-endian i32 at `offset`.",
            __RTS_FN_NS_BUFFER_READ_I32 as *const u8,
            false,
        ))
        .member(func(
            "read_i64",
            "__RTS_FN_NS_BUFFER_READ_I64",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "read_i64(handle: number, offset: number): number",
            "Reads a little-endian i64 at `offset`.",
            __RTS_FN_NS_BUFFER_READ_I64 as *const u8,
            false,
        ))
        .member(func(
            "read_f64",
            "__RTS_FN_NS_BUFFER_READ_F64",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::F64),
            "read_f64(handle: number, offset: number): number",
            "Reads a little-endian f64 at `offset`. NaN out of bounds.",
            __RTS_FN_NS_BUFFER_READ_F64 as *const u8,
            false,
        ))
        .member(func(
            "read_f32",
            "__RTS_FN_NS_BUFFER_READ_F32",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::F64),
            "read_f32(handle: number, offset: number): number",
            "Reads a little-endian f32 at `offset`, widened to number. NaN out of bounds.",
            __RTS_FN_NS_BUFFER_READ_F32 as *const u8,
            false,
        ))
        .member(func(
            "write_u8",
            "__RTS_FN_NS_BUFFER_WRITE_U8",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::I32], AbiType::Void),
            "write_u8(handle: number, offset: number, val: number): void",
            "Writes `val` as u8 at `offset`. No-op out of bounds.",
            __RTS_FN_NS_BUFFER_WRITE_U8 as *const u8,
            false,
        ))
        .member(func(
            "write_i32",
            "__RTS_FN_NS_BUFFER_WRITE_I32",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::I32], AbiType::Void),
            "write_i32(handle: number, offset: number, val: number): void",
            "Writes a little-endian i32 at `offset`.",
            __RTS_FN_NS_BUFFER_WRITE_I32 as *const u8,
            false,
        ))
        .member(func(
            "write_i64",
            "__RTS_FN_NS_BUFFER_WRITE_I64",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::I64], AbiType::Void),
            "write_i64(handle: number, offset: number, val: number): void",
            "Writes a little-endian i64 at `offset`.",
            __RTS_FN_NS_BUFFER_WRITE_I64 as *const u8,
            false,
        ))
        .member(func(
            "write_f64",
            "__RTS_FN_NS_BUFFER_WRITE_F64",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::F64], AbiType::Void),
            "write_f64(handle: number, offset: number, val: number): void",
            "Writes a little-endian f64 at `offset`.",
            __RTS_FN_NS_BUFFER_WRITE_F64 as *const u8,
            false,
        ))
        .member(func(
            "write_f32",
            "__RTS_FN_NS_BUFFER_WRITE_F32",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::F64], AbiType::Void),
            "write_f32(handle: number, offset: number, val: number): void",
            "Writes `val` as a little-endian f32 at `offset` (narrowed from number). No-op out of bounds. Ideal for audio sample buffers.",
            __RTS_FN_NS_BUFFER_WRITE_F32 as *const u8,
            false,
        ))
        .member(func(
            "copy",
            "__RTS_FN_NS_BUFFER_COPY",
            Sig::new(
                vec![AbiType::U64, AbiType::I64, AbiType::U64, AbiType::I64, AbiType::I64],
                AbiType::Void,
            ),
            "copy(dst: number, dstOff: number, src: number, srcOff: number, len: number): void",
            "Copies `len` bytes from src+srcOff to dst+dstOff. Safe with overlapping src/dst.",
            __RTS_FN_NS_BUFFER_COPY as *const u8,
            false,
        ))
        .member(func(
            "fill",
            "__RTS_FN_NS_BUFFER_FILL",
            Sig::new(vec![AbiType::U64, AbiType::I32, AbiType::I64], AbiType::Void),
            "fill(handle: number, byte: number, len: number): void",
            "Fills the first `len` bytes with `byte`.",
            __RTS_FN_NS_BUFFER_FILL as *const u8,
            false,
        ))
        .member(func(
            "to_string",
            "__RTS_FN_NS_BUFFER_TO_STRING",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "to_string(handle: number): number",
            "Interprets buffer contents as UTF-8 and returns a string handle.",
            __RTS_FN_NS_BUFFER_TO_STRING as *const u8,
            false,
        ))
        .member(func(
            "equals",
            "__RTS_FN_NS_BUFFER_EQUALS",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Bool),
            "equals(a: number, b: number): boolean",
            "Byte-wise equality of two buffers.",
            __RTS_FN_NS_BUFFER_EQUALS as *const u8,
            true,
        ))
        .member(func(
            "index_of",
            "__RTS_FN_NS_BUFFER_INDEX_OF",
            Sig::new(vec![AbiType::U64, AbiType::I32, AbiType::I64], AbiType::I64),
            "index_of(handle: number, byte: number, from: number): number",
            "Index of first occurrence of byte starting at `from`. Returns -1 if not found.",
            __RTS_FN_NS_BUFFER_INDEX_OF as *const u8,
            true,
        ))
        .done();
}

// ── DataView (big-endian por padrao, igual JS) ──────────────────────────────
// Metodos de classes globais (DataView/ArrayBuffer/TypedArray/Atomics) backed
// por Entry::Buffer. NAO sao membros do namespace `buffer`; codegen chama por
// simbolo. Ficam como free externs.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_UINT8(handle: u64, offset: i64, val: i64) {
    with_buffer_mut(handle, (), |b| {
        if offset >= 0 {
            if let Some(s) = b.get_mut(offset as usize) {
                *s = val as u8;
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_UINT8(handle: u64, offset: i64) -> i64 {
    with_buffer(handle, 0, |b| {
        if offset >= 0 {
            b.get(offset as usize).map(|&v| v as i64).unwrap_or(0)
        } else {
            0
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_UINT16(handle: u64, offset: i64, val: i64) {
    write_bytes_at(handle, offset, &(val as u16).to_be_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_UINT16(handle: u64, offset: i64) -> i64 {
    read_bytes_at::<2>(handle, offset)
        .map(|b| u16::from_be_bytes(b) as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_INT32(handle: u64, offset: i64, val: i64) {
    write_bytes_at(handle, offset, &(val as i32).to_be_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_INT32(handle: u64, offset: i64) -> i64 {
    read_bytes_at::<4>(handle, offset)
        .map(|b| i32::from_be_bytes(b) as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_BYTE_LENGTH(handle: u64) -> i64 {
    with_buffer(handle, 0, |b| b.len() as i64)
}

fn write_bytes_at(handle: u64, offset: i64, bytes: &[u8]) {
    with_buffer_mut(handle, (), |b| {
        if offset < 0 {
            return;
        }
        let start = offset as usize;
        for (i, &byte) in bytes.iter().enumerate() {
            if let Some(s) = b.get_mut(start + i) {
                *s = byte;
            }
        }
    });
}

fn read_bytes_at<const N: usize>(handle: u64, offset: i64) -> Option<[u8; N]> {
    with_buffer(handle, None, |b| {
        if offset < 0 {
            return None;
        }
        let start = offset as usize;
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = *b.get(start + i)?;
        }
        Some(out)
    })
}

/// `new ArrayBuffer(size)` — aloca um buffer de bytes zerado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_BUFFER_NEW(size: i64) -> u64 {
    __RTS_FN_NS_BUFFER_ALLOC(size)
}

/// `arrayBuffer.slice(start, end)` — novo ArrayBuffer com a cópia dos bytes
/// no range [start, end). Indices negativos contam do fim (JS spec). `end`
/// omitido (sentinela i64::MIN) = ate o fim.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_BUFFER_SLICE(handle: u64, start: i64, end: i64) -> u64 {
    let bytes: Vec<u8> = with_buffer(handle, Vec::new(), |b| {
        let len = b.len() as i64;
        let norm = |i: i64| -> i64 {
            let v = if i < 0 { len + i } else { i };
            v.clamp(0, len)
        };
        let s = norm(start);
        let e = if end == i64::MIN { len } else { norm(end) };
        if e <= s {
            Vec::new()
        } else {
            b[s as usize..e as usize].to_vec()
        }
    });
    alloc_entry(Entry::Buffer(bytes))
}

/// `new DataView(buffer)` — view sobre o ArrayBuffer (byteOffset 0). O
/// handle do DataView eh o proprio handle do buffer (sem offset/length
/// parciais por enquanto).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_NEW(buffer: u64) -> u64 {
    buffer
}

/// `dataView.byteOffset` — sempre 0 nesta implementacao (view cobre todo
/// o buffer).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_BYTE_OFFSET(_handle: u64) -> i64 {
    0
}

// ── DataView com flag littleEndian (overloads 3-arg get / 4-arg set) ────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_UINT16_LE(
    handle: u64,
    offset: i64,
    val: i64,
    little_endian: i32,
) {
    let be = (val as u16).to_be_bytes();
    let bytes = if little_endian != 0 {
        [be[1], be[0]]
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_UINT16_LE(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> i64 {
    read_bytes_at::<2>(handle, offset)
        .map(|b| if little_endian != 0 { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) } as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_INT32_LE(
    handle: u64,
    offset: i64,
    val: i64,
    little_endian: i32,
) {
    let be = (val as i32).to_be_bytes();
    let bytes = if little_endian != 0 {
        [be[3], be[2], be[1], be[0]]
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_INT32_LE(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> i64 {
    read_bytes_at::<4>(handle, offset)
        .map(|b| if little_endian != 0 { i32::from_le_bytes(b) } else { i32::from_be_bytes(b) } as i64)
        .unwrap_or(0)
}

// Floats — sempre recebem o flag (fixtures usam littleEndian explicito).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_FLOAT64(
    handle: u64,
    offset: i64,
    val: f64,
    little_endian: i32,
) {
    let be = val.to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be;
        le.reverse();
        le
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_FLOAT64(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> f64 {
    read_bytes_at::<8>(handle, offset)
        .map(|b| {
            if little_endian != 0 {
                f64::from_le_bytes(b)
            } else {
                f64::from_be_bytes(b)
            }
        })
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_FLOAT32(
    handle: u64,
    offset: i64,
    val: f64,
    little_endian: i32,
) {
    let be = (val as f32).to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be;
        le.reverse();
        le
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_FLOAT32(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> f64 {
    read_bytes_at::<4>(handle, offset)
        .map(|b| {
            (if little_endian != 0 {
                f32::from_le_bytes(b)
            } else {
                f32::from_be_bytes(b)
            }) as f64
        })
        .unwrap_or(0.0)
}

// ── DataView BigInt64/BigUint64 (#65) — 8-byte int, littleEndian flag ────────
// BigInt cruza o JIT como i64 cru (mesma convencao dos literais `1n`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_BIGINT64(
    handle: u64,
    offset: i64,
    val: i64,
    little_endian: i32,
) {
    let be = val.to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be;
        le.reverse();
        le
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_BIGINT64(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> i64 {
    read_bytes_at::<8>(handle, offset)
        .map(|b| {
            if little_endian != 0 {
                i64::from_le_bytes(b)
            } else {
                i64::from_be_bytes(b)
            }
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_BIGUINT64(
    handle: u64,
    offset: i64,
    val: i64,
    little_endian: i32,
) {
    let be = (val as u64).to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be;
        le.reverse();
        le
    } else {
        be
    };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_BIGUINT64(
    handle: u64,
    offset: i64,
    little_endian: i32,
) -> i64 {
    read_bytes_at::<8>(handle, offset)
        .map(|b| if little_endian != 0 { u64::from_le_bytes(b) } else { u64::from_be_bytes(b) } as i64)
        .unwrap_or(0)
}

// ── TypedArray view sobre ArrayBuffer (#811/205) ────────────────────────────

/// Le `view[index]` de um Entry::Buffer. `elem_bytes` = 1/2/4/8; `signed`!=0
/// estende sinal; `is_float`!=0 retorna bits f64 (4=f32->f64, 8=f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_GET_ELEM(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    signed: i64,
    is_float: i64,
) -> i64 {
    with_buffer(handle, 0, |b| {
        if index < 0 {
            return 0;
        }
        let off = (index as usize) * (elem_bytes as usize);
        if off + (elem_bytes as usize) > b.len() {
            return 0;
        }
        let bytes = &b[off..off + elem_bytes as usize];
        if is_float != 0 {
            return match elem_bytes {
                4 => {
                    let arr: [u8; 4] = bytes.try_into().unwrap();
                    (f32::from_le_bytes(arr) as f64).to_bits() as i64
                }
                8 => {
                    let arr: [u8; 8] = bytes.try_into().unwrap();
                    f64::from_le_bytes(arr).to_bits() as i64
                }
                _ => 0,
            };
        }
        let mut v: u64 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            v |= (byte as u64) << (i * 8);
        }
        if signed != 0 {
            // estende sinal a partir do bit mais alto de elem_bytes.
            let bits = (elem_bytes as u32) * 8;
            let shift = 64 - bits;
            return ((v << shift) as i64) >> shift;
        }
        v as i64
    })
}

/// Escreve `view[index] = val` num Entry::Buffer (little-endian).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_SET_ELEM(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    is_float: i64,
    val: i64,
) {
    with_buffer_mut(handle, (), |b| {
        if index < 0 {
            return;
        }
        let off = (index as usize) * (elem_bytes as usize);
        if off + (elem_bytes as usize) > b.len() {
            return;
        }
        let raw: [u8; 8] = if is_float != 0 {
            match elem_bytes {
                4 => {
                    let f = f64::from_bits(val as u64) as f32;
                    let le = f.to_le_bytes();
                    [le[0], le[1], le[2], le[3], 0, 0, 0, 0]
                }
                8 => f64::from_bits(val as u64).to_le_bytes(),
                _ => [0; 8],
            }
        } else {
            (val as u64).to_le_bytes()
        };
        for i in 0..(elem_bytes as usize) {
            b[off + i] = raw[i];
        }
    });
}

/// (#69) Atomics.* sobre um TypedArray-view inteiro (backing
/// (Shared)ArrayBuffer). O RTS roda single-thread, entao cada operacao eh
/// um read-modify-write nao-concorrente — semanticamente identica ao
/// resultado observavel de Atomics em JS sem contencao real. `op` codifica
/// a operacao; `signed` controla extensao de sinal na leitura do valor
/// anterior (retornado por add/sub/and/or/xor/exchange/compareExchange).
///
/// op: 0=add 1=sub 2=and 3=or 4=xor 5=exchange (store+ret prev)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ATOMICS_RMW(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    signed: i64,
    op: i64,
    operand: i64,
) -> i64 {
    let prev = __RTS_FN_GL_TA_GET_ELEM(handle, index, elem_bytes, signed, 0);
    let next = match op {
        0 => prev.wrapping_add(operand),
        1 => prev.wrapping_sub(operand),
        2 => prev & operand,
        3 => prev | operand,
        4 => prev ^ operand,
        5 => operand, // exchange: store operand, retorna prev
        _ => prev,
    };
    __RTS_FN_GL_TA_SET_ELEM(handle, index, elem_bytes, 0, next);
    prev
}

/// (#69) Atomics.compareExchange(view, idx, expected, replacement). Se o
/// valor atual == expected, grava replacement. Retorna SEMPRE o valor
/// anterior (spec JS), tenha trocado ou nao.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ATOMICS_CAS(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    signed: i64,
    expected: i64,
    replacement: i64,
) -> i64 {
    let prev = __RTS_FN_GL_TA_GET_ELEM(handle, index, elem_bytes, signed, 0);
    if prev == expected {
        __RTS_FN_GL_TA_SET_ELEM(handle, index, elem_bytes, 0, replacement);
    }
    prev
}

/// (#69) Atomics.load(view, idx) — leitura simples (signed-aware).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ATOMICS_LOAD(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    signed: i64,
) -> i64 {
    __RTS_FN_GL_TA_GET_ELEM(handle, index, elem_bytes, signed, 0)
}

/// (#69) Atomics.store(view, idx, value) — grava e retorna value (spec JS).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ATOMICS_STORE(
    handle: u64,
    index: i64,
    elem_bytes: i64,
    value: i64,
) -> i64 {
    __RTS_FN_GL_TA_SET_ELEM(handle, index, elem_bytes, 0, value);
    value
}

/// (#68) `view.set(srcArray, offset)` onde `view` eh uma TypedArray-view
/// sobre (Shared)ArrayBuffer. Escreve cada elemento de `src` (Vec<i64>) como
/// `elem_bytes` bytes little-endian no buffer, a partir de `offset` (em
/// elementos). Reusa TA_SET_ELEM para respeitar signed/float e bounds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_SET_FROM(
    handle: u64,
    src: u64,
    offset: i64,
    elem_bytes: i64,
    is_float: i64,
) {
    if offset < 0 {
        return;
    }
    let items: Vec<i64> = with_entry(src, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    for (i, x) in items.into_iter().enumerate() {
        __RTS_FN_GL_TA_SET_ELEM(handle, offset + i as i64, elem_bytes, is_float, x);
    }
}

/// `typedArray.length` quando o backing eh um ArrayBuffer: byteLength/elem_bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_LENGTH(handle: u64, elem_bytes: i64) -> i64 {
    with_buffer(handle, 0, |b| {
        if elem_bytes <= 0 {
            return 0;
        }
        (b.len() as i64) / elem_bytes
    })
}

/// (#68) Detach de ArrayBuffer (transferencia via structuredClone
/// `{transfer:[buf]}`). JS spec: o buffer fonte fica detached — byteLength
/// vira 0 e views sobre ele leem 0. RTS modela truncando o Buffer a vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BUFFER_DETACH(handle: u64) {
    with_buffer_mut(handle, (), |b| b.clear());
}
