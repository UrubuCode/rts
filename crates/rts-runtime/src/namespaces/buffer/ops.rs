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

// ── DataView (big-endian por padrao, igual JS) ──────────────────────────────
// DataView opera sobre um Entry::Buffer. Os getters/setters seguem a spec JS:
// big-endian quando `little_endian == 0`. Cobre cross-runtime 206_dataview_basic.

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
    read_bytes_at::<2>(handle, offset).map(|b| u16::from_be_bytes(b) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_INT32(handle: u64, offset: i64, val: i64) {
    write_bytes_at(handle, offset, &(val as i32).to_be_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_INT32(handle: u64, offset: i64) -> i64 {
    read_bytes_at::<4>(handle, offset).map(|b| i32::from_be_bytes(b) as i64).unwrap_or(0)
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
// JS: get/set aceitam `littleEndian` opcional. Registramos membros separados
// por aridade (sem flag = big-endian default; com flag = honra o booleano).

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_UINT16_LE(handle: u64, offset: i64, val: i64, little_endian: i32) {
    let be = (val as u16).to_be_bytes();
    let bytes = if little_endian != 0 { [be[1], be[0]] } else { be };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_UINT16_LE(handle: u64, offset: i64, little_endian: i32) -> i64 {
    read_bytes_at::<2>(handle, offset)
        .map(|b| if little_endian != 0 { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) } as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_INT32_LE(handle: u64, offset: i64, val: i64, little_endian: i32) {
    let be = (val as i32).to_be_bytes();
    let bytes = if little_endian != 0 { [be[3], be[2], be[1], be[0]] } else { be };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_INT32_LE(handle: u64, offset: i64, little_endian: i32) -> i64 {
    read_bytes_at::<4>(handle, offset)
        .map(|b| if little_endian != 0 { i32::from_le_bytes(b) } else { i32::from_be_bytes(b) } as i64)
        .unwrap_or(0)
}

// Floats — sempre recebem o flag (fixtures usam littleEndian explicito).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_FLOAT64(handle: u64, offset: i64, val: f64, little_endian: i32) {
    let be = val.to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be; le.reverse(); le
    } else { be };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_FLOAT64(handle: u64, offset: i64, little_endian: i32) -> f64 {
    read_bytes_at::<8>(handle, offset)
        .map(|b| if little_endian != 0 { f64::from_le_bytes(b) } else { f64::from_be_bytes(b) })
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_SET_FLOAT32(handle: u64, offset: i64, val: f64, little_endian: i32) {
    let be = (val as f32).to_be_bytes();
    let bytes = if little_endian != 0 {
        let mut le = be; le.reverse(); le
    } else { be };
    write_bytes_at(handle, offset, &bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATAVIEW_GET_FLOAT32(handle: u64, offset: i64, little_endian: i32) -> f64 {
    read_bytes_at::<4>(handle, offset)
        .map(|b| (if little_endian != 0 { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) }) as f64)
        .unwrap_or(0.0)
}

// ── TypedArray view sobre ArrayBuffer (#811/205) ────────────────────────────
// `new Uint8Array(buf)` etc. retornam o proprio handle do buffer; o codegen
// rastreia a largura/sinal do elemento por-var e chama estes helpers para
// `view[i]` (little-endian, igual a plataforma JS). index eh em ELEMENTOS.

/// Le `view[index]` de um Entry::Buffer. `elem_bytes` = 1/2/4/8; `signed`!=0
/// estende sinal; `is_float`!=0 retorna bits f64 (4=f32->f64, 8=f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_GET_ELEM(handle: u64, index: i64, elem_bytes: i64, signed: i64, is_float: i64) -> i64 {
    with_buffer(handle, 0, |b| {
        if index < 0 { return 0; }
        let off = (index as usize) * (elem_bytes as usize);
        if off + (elem_bytes as usize) > b.len() { return 0; }
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
pub extern "C" fn __RTS_FN_GL_TA_SET_ELEM(handle: u64, index: i64, elem_bytes: i64, is_float: i64, val: i64) {
    with_buffer_mut(handle, (), |b| {
        if index < 0 { return; }
        let off = (index as usize) * (elem_bytes as usize);
        if off + (elem_bytes as usize) > b.len() { return; }
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

/// `typedArray.length` quando o backing eh um ArrayBuffer: byteLength/elem_bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TA_LENGTH(handle: u64, elem_bytes: i64) -> i64 {
    with_buffer(handle, 0, |b| {
        if elem_bytes <= 0 { return 0; }
        (b.len() as i64) / elem_bytes
    })
}
