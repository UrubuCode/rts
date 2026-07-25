//! Impl (parcial) da classe primordial `ArrayBuffer`. `ArrayBuffer` é
//! PRIMORDIAL (CLAUDE.md doctrine, reclassificado 2026-07-03): define/molda o
//! MODELO DE MEMÓRIA cru que TypedArrays indexam — o motor pode nomeá-la
//! direto.
//!
//! (LAYERING FIX 2026-07-24) `__RTS_FN_GL_ARRAY_BUFFER_NEW`/`_SLICE` MUDARAM
//! de `rts-shared/buffer/mod.rs` pra cá: só tocam `Entry::Buffer(Vec<u8>)`,
//! definido em `rts-engine` — nenhum estado non-primordial. O resto do
//! namespace `buffer` (`__RTS_FN_NS_BUFFER_*`, raw byte ops) FICA em
//! rts-shared: é a superfície `rts:buffer` de baixo nível, não a classe
//! `ArrayBuffer` em si. `__RTS_FN_GL_ARRAY_BUFFER_NEW` antes delegava em
//! `rts-shared`'s `__RTS_FN_NS_BUFFER_ALLOC` — para não puxar rts-shared
//! (proibido pela direção do grafo de crates), a alocação (1 linha, zero
//! estado extra) é reimplementada aqui direto sobre `Entry::Buffer`.

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

fn with_buffer<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<u8>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Buffer(buf)) => f(buf),
        _ => default,
    })
}

/// `new ArrayBuffer(size)` — aloca um buffer de bytes zerado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_BUFFER_NEW(size: i64) -> u64 {
    if size < 0 {
        return 0;
    }
    alloc_entry(Entry::Buffer(vec![0u8; size as usize]))
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
