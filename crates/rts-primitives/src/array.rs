//! Spec + impl (parcial) da classe primordial `Array`. A maioria dos corpos
//! `#[no_mangle]` (símbolos `__RTS_FN_NS_COLLECTIONS_VEC_*`) ficam em
//! `rts-shared/collections/vec.rs` — dispatch real de `arr.method()` é
//! hardcoded no front-end (`crates/rts-codegen-new/src/front/run/`). O builder
//! `register_array_class_spec` foi removido em DRAIN_MOTOR: nunca era chamado
//! por nenhum path de REGISTER/populate do motor.
//!
//! (LAYERING FIX 2026-07-24) As 2 fns abaixo (`__RTS_FN_GL_ARRAY_FROM_LENGTH`/
//! `__RTS_FN_GL_ARRAY_NEW_WITH_LENGTH`) MUDARAM de `rts-shared/collections/
//! vec.rs` pra cá — não tocam nenhum estado/Entry non-primordial (só
//! `Entry::Vec`, definido em `rts-engine`), então cabem aqui sem puxar
//! rts-shared. `__RTS_FN_GL_ARRAY_FROM_VEC` FICOU em rts-shared: depende de
//! `handle_is_set_kind`/`handle_is_map_kind`/`set_element_from_pair`
//! (Map/Set — non-primordial) e do generator-drain de `gc_surface`; mover
//! puxaria uma dependência rts-primitives → rts-shared (proibida).

use rts_engine::heap::handles::{Entry, alloc_entry};

/// Mesmo limite de `rts-shared::collections::vec::VEC_MAX_LEN` — evita OOM em
/// `new Array(len)`/`Array.from({length})` com `len` hostil.
const VEC_MAX_LEN: i64 = 1_000_000;

/// (#780/#786) `new Array(len)` — cria Vec preenchido com sentinel de HOLE
/// `i64::MIN + 4` (cross-runtime #1533): JS spec cria slots vazios (holes),
/// nao undefined — `0 in new Array(3)` eh false. Leitura `a[0] === undefined`
/// continua true (codegen trata MIN+4 como undefined-equivalente). Limitado
/// ao `VEC_MAX_LEN` pra evitar OOM.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_NEW_WITH_LENGTH(len: i64) -> u64 {
    let len = len.max(0).min(VEC_MAX_LEN) as usize;
    let v = vec![i64::MIN + 4; len];
    alloc_entry(Entry::Vec(Box::new(v)))
}

/// (#208) `Array.from({length: n}, fn?)` — gera Vec [fn(0), fn(1), ...].
/// Se fn_ptr == 0, gera [0, 1, ..., n-1] (sem mapeamento).
/// fn_ptr e' `extern "C" fn(item: i64, idx: i64) -> i64`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM_LENGTH(n: i64, fn_ptr: u64) -> u64 {
    if n < 0 || n > VEC_MAX_LEN {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    }
    let n = n as usize;
    let mut out: Vec<i64> = Vec::with_capacity(n);
    if fn_ptr == 0 {
        for i in 0..n {
            out.push(i as i64);
        }
    } else {
        // SAFETY: fn_ptr e' `extern "C" fn(i64, i64) -> i64`.
        // Em JS Array.from(arrayLike, mapFn) chama mapFn(undefined, idx)
        // pra cada slot vazio. Aqui passamos (idx, idx) pra simplificar
        // — mapper tipico em RTS recebe `(_, i) => ...` e usa so' i.
        let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        for i in 0..n {
            out.push(f(i as i64, i as i64));
        }
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}
