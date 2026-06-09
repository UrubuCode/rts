//! Vec<i64> — lista ordenada de valores i64.
//!
//! Os 9 membros do namespace `collections` referentes a Vec vivem no
//! `#[rts_namespace(collections, part)] impl CollectionsVecNs` abaixo (stage
//! 2c, `docs/specs/rts-core-engine.md`); o resto sao non-member externs
//! (`*_AUTO`, `GL_ARRAY_*`, Array.prototype methods) que o codegen chama por
//! simbolo. `mod.rs` junta os MEMBERS de map+vec via `rts_abi::concat_members`.

use rts_abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

use super::super::gc::handles::{alloc_entry, free_handle, with_entry, with_entry_mut, Entry};

fn with_vec<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v.as_ref()),
        _ => default,
    })
}

fn with_vec_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Vec<i64>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v.as_mut()),
        _ => default,
    })
}

/// Membros Vec do namespace `collections` (parte; map.rs declara a outra).
#[rts_namespace(collections, part)]
impl CollectionsVecNs {
    /// Creates an empty Vec<number>.
    #[rts_fn]
    pub fn vec_new() -> Handle {
        alloc_entry(Entry::Vec(Box::new(Vec::new())))
    }

    /// Releases the vec handle.
    #[rts_fn]
    pub fn vec_free(h: U64) {
        free_handle(h);
    }

    /// Number of elements; -1 if the handle is invalid.
    #[rts_fn]
    pub fn vec_len(h: U64) -> I64 {
        with_vec(h, -1, |v| v.len() as i64)
    }

    /// Appends `value` to the end.
    #[rts_fn]
    pub fn vec_push(h: U64, value: I64) {
        let limit_hit = with_vec_mut(h, false, |v| {
            if v.len() >= VEC_MAX_LEN {
                return true;
            }
            v.push(value);
            false
        });
        if limit_hit {
            eprintln!(
                "RTS runtime: vec push exceeded limit of {VEC_MAX_LEN} elements; aborting (likely infinite generator or unbounded loop)"
            );
            std::process::abort();
        }
    }

    /// Removes and returns the last element; 0 when empty.
    #[rts_fn]
    pub fn vec_pop(h: U64) -> I64 {
        let popped: Option<i64> = with_vec_mut(h, None, |v| v.pop());
        match popped {
            Some(v) => v,
            None => i64::MIN + 2,
        }
    }

    /// Element at `index`, or 0 out of range.
    #[rts_fn]
    pub fn vec_get(h: U64, index: I64) -> I64 {
        if index < 0 {
            return 0;
        }
        with_vec(h, 0, |v| v.get(index as usize).copied().unwrap_or(0))
    }

    /// Writes `value` at `index`. No-op out of range.
    #[rts_fn]
    pub fn vec_set(h: U64, index: I64, value: I64) {
        if index < 0 {
            return;
        }
        with_vec_mut(h, (), |v| {
            if let Some(slot) = v.get_mut(index as usize) {
                *slot = value;
            }
        });
    }

    /// Removes all elements.
    #[rts_fn]
    pub fn vec_clear(h: U64) {
        with_vec_mut(h, (), |v| v.clear());
    }

    /// Junta os elementos do vec separados por `sep` (string handle). Cada elemento i64 e' tratado como string handle se valido, senao formatado como numero decimal. Retorna handle da string resultante.
    #[rts_fn]
    pub fn vec_join(h: U64, sep: Handle) -> Handle {
        let elems: Option<Vec<i64>> = with_entry(h, |entry| match entry {
            Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
            _ => None,
        });
        let Some(elems) = elems else {
            return 0;
        };
        let sep_bytes: Vec<u8> = with_entry(sep, |entry| match entry {
            Some(Entry::String(b)) => b.clone(),
            _ => Vec::new(),
        });
        let mut out: Vec<u8> = Vec::new();
        join_into(&mut out, &elems, &sep_bytes, 0);
        alloc_entry(Entry::String(out))
    }
}

/// (cross-runtime #1067) Suporte a spread em indirect call: copia todos
/// os elementos de `src` para `dst`. Usado pelo codegen para `f(...args)`
/// onde args eh Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM(dst: u64, src: u64) {
    // (cross-runtime #79) src pode ser Vec OU Buffer (TextEncoder.encode,
    // digest). Antes so' lia Vec — fonte Buffer dava Vec vazio.
    let items: Vec<i64> = with_entry(src, |entry| match entry {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        Some(Entry::Buffer(b)) => b.iter().map(|&x| x as i64).collect(),
        _ => Vec::new(),
    });
    with_vec_mut(dst, (), |v| {
        for x in items {
            if v.len() >= 1_000_000 {
                break;
            }
            v.push(x);
        }
    });
}

/// (cross-runtime #79) `new TypedArray(arg)` quando `arg` tem tipo ambiguo
/// (ex: resultado de `await`): decide em RUNTIME se eh um handle (Buffer/Vec ->
/// copia elementos) ou um comprimento (-> N zeros). Resolve
/// `new Uint8Array(await crypto.subtle.digest(...))` sem o codegen saber o tipo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FILL_TA_ARG(dst: u64, arg: i64) {
    let h = arg as u64;
    let from_handle: Option<Vec<i64>> = with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) => Some(b.iter().map(|&x| x as i64).collect()),
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    });
    with_vec_mut(dst, (), |v| match from_handle {
        Some(src) => {
            for x in src {
                if v.len() >= 1_000_000 {
                    break;
                }
                v.push(x);
            }
        }
        None => {
            let n = arg.max(0).min(1_000_000);
            for _ in 0..n {
                v.push(0);
            }
        }
    });
}

/// (#57/#205) `new Uint8Array(arrayBuffer)` — materializa um Vec<i64> com os
/// BYTES do ArrayBuffer (`Entry::Buffer`), um byte por slot. Snapshot de
/// leitura (escritas posteriores no Vec nao refletem no buffer; view viva
/// real fica como follow-up).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM_BUFFER(dst: u64, src: u64) {
    let bytes: Vec<u8> = with_entry(src, |entry| match entry {
        Some(Entry::Buffer(b)) => b.clone(),
        _ => Vec::new(),
    });
    with_vec_mut(dst, (), |v| {
        for b in bytes {
            if v.len() >= 1_000_000 {
                break;
            }
            v.push(b as i64);
        }
    });
}

/// Limite duro de elementos por vec — protege contra OOM em cenarios
/// patologicos (ex: generator infinito desugared para buffer eager,
/// loop sem condicao de parada). 1M i64 = 8MiB por vec, suficiente
/// pro caso real e barato comparado aos GBs que um leak descontrolado
/// produz.
const VEC_MAX_LEN: usize = 1_000_000;

/// (cross-runtime) `Math.min(...arr)` / `Math.max(...arr)`. Reduz os elementos
/// do Vec (i64) como f64. Vec vazio -> Infinity (min) / -Infinity (max), JS spec.
/// Retorna f64 (codegen reinterpreta).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_MIN(handle: u64) -> f64 {
    with_vec(handle, f64::INFINITY, |v| {
        v.iter().fold(f64::INFINITY, |acc, &x| acc.min(x as f64))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_MAX(handle: u64) -> f64 {
    with_vec(handle, f64::NEG_INFINITY, |v| {
        v.iter()
            .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x as f64))
    })
}

/// `arr.length = n` — JS spec: trunca se n < length, extende com
/// undefined (slot 0) se n > length, no-op se n == length.
/// n negativo gera RangeError em JS; aqui silently no-op (RTS nao
/// tem throw real; alternativa seria abort).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SET_LENGTH(handle: u64, n: i64) {
    if n < 0 {
        return;
    }
    let n = n as usize;
    with_vec_mut(handle, (), |v| {
        if n < v.len() {
            v.truncate(n);
        } else if n > v.len() {
            // JS extension: novos slots viram `undefined`. Usa sentinel
            // i64::MIN+2 (reconhecido pelo codegen em `=== undefined`)
            // em vez de handle de string "undefined" — handle nao casa
            // com sentinel e arr[i] === undefined retornaria false
            // (#861 fixture 214).
            let needed = n - v.len();
            for _ in 0..needed {
                v.push(i64::MIN + 2);
            }
        }
    });
}

/// `delete obj[i]` ou `delete obj[k]` — roteia em runtime para vec/map.
/// Para Vec: marca slot como hole (i64::MIN+4) preservando length (JS spec).
/// Para Map: remove key (idx_or_keyh interpretado como handle de string).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_INDEX_DELETE_AUTO(handle: u64, idx_or_keyh: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    enum Kind {
        Vec,
        Map,
        Other,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Vec(_)) => Kind::Vec,
        Some(Entry::Map(_)) => Kind::Map,
        _ => Kind::Other,
    });
    match kind {
        Kind::Vec => {
            let i = idx_or_keyh;
            if i < 0 {
                return 1;
            }
            with_entry_mut(handle, |e| {
                if let Some(Entry::Vec(v)) = e {
                    if (i as usize) < v.len() {
                        v[i as usize] = i64::MIN + 4; // hole sentinel
                    }
                }
                1i64
            })
        }
        Kind::Map => {
            let key_owned: Option<String> = with_entry(idx_or_keyh as u64, |e| match e {
                Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
                _ => None,
            });
            let Some(key) = key_owned else { return 1 };
            with_entry_mut(handle, |e| {
                if let Some(Entry::Map(m)) = e {
                    let _ = m.shift_remove(&key);
                }
                1i64
            })
        }
        Kind::Other => 1,
    }
}

/// `obj[i]` auto: se handle for Vec, devolve slot; se String, devolve char-at
/// (handle de string single-char ou handle de "undefined" out-of-range — JS spec).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_INDEX_GET_AUTO(handle: u64, index: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    enum Kind {
        Vec,
        Str(Vec<u8>),
        Buf(u8),
        Other,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Vec(_)) => Kind::Vec,
        Some(Entry::String(b)) => Kind::Str(b.clone()),
        Some(Entry::Buffer(b)) => {
            if index < 0 || (index as usize) >= b.len() {
                Kind::Other
            } else {
                Kind::Buf(b[index as usize])
            }
        }
        _ => Kind::Other,
    });
    match kind {
        // (#58) Buffer indexing: bytes[i] retorna byte como i64.
        Kind::Buf(byte) => byte as i64,
        Kind::Vec => __RTS_FN_NS_COLLECTIONS_VEC_GET(handle, index),
        Kind::Str(bytes) => {
            if index < 0 {
                return alloc_entry(Entry::String(b"undefined".to_vec())) as i64;
            }
            let Ok(s) = std::str::from_utf8(&bytes) else {
                return alloc_entry(Entry::String(b"undefined".to_vec())) as i64;
            };
            match s.chars().nth(index as usize) {
                Some(c) => {
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf).as_bytes().to_vec();
                    alloc_entry(Entry::String(encoded)) as i64
                }
                None => alloc_entry(Entry::String(b"undefined".to_vec())) as i64,
            }
        }
        Kind::Other => {
            // (cross-runtime #340) Map handle: lookup com key string. O `index`
            // pode ser:
            // - String handle (computed key `obj[key]` onde key="a") -> usar MAP_GET_KH.
            // - i64 raw (numeric index `arr[0]` aplicado a Map result) -> usar
            //   index.to_string() como key.
            use super::map::{__RTS_FN_NS_COLLECTIONS_MAP_GET, __RTS_FN_NS_COLLECTIONS_MAP_GET_KH};
            // Detecta se index eh handle GC valido String/Symbol — usa MAP_GET_KH.
            let is_str_handle = with_entry(index as u64, |e| {
                matches!(e, Some(Entry::String(_)) | Some(Entry::Symbol { .. }))
            });
            if is_str_handle {
                return __RTS_FN_NS_COLLECTIONS_MAP_GET_KH(handle, index as u64);
            }
            let key = index.to_string();
            let key_bytes = key.as_bytes();
            __RTS_FN_NS_COLLECTIONS_MAP_GET(handle, key_bytes.as_ptr(), key_bytes.len() as i64)
        }
    }
}

/// (cross-runtime #808) `recv.concat(other)` despachado em runtime:
/// se recv eh Vec, faz vec concat (copy + append/spread); se string,
/// faz string concat. Usado quando recv eh I64 ambiguo (param de
/// arrow lifted em reduceRight/reduce).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_CONCAT_AUTO(recv: u64, other: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    let is_vec = with_entry(recv, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        let copy = __RTS_FN_NS_COLLECTIONS_VEC_CONCAT(recv, 0);
        __RTS_FN_NS_COLLECTIONS_VEC_CONCAT_APPEND(copy, other) as i64
    } else {
        crate::namespaces::globals::string::rt::__RTS_FN_GL_STRING_CONCAT(recv, other as u64) as i64
    }
}

/// (cross-runtime #285) Runtime dispatch para `recv.slice(start, end)` quando
/// recv pode ser Vec ou String em compile time (capture em arrow body).
/// end == i64::MIN significa "ate o final" (codegen sentinel quando user
/// chama .slice(n) sem end).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SLICE_AUTO(recv: u64, start: i64, end: i64) -> u64 {
    use super::super::gc::handles::with_entry;
    let is_vec = with_entry(recv, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        __RTS_FN_NS_COLLECTIONS_VEC_SLICE(recv, start, end)
    } else {
        // STRING_SLICE nao tem sentinel pra "end of string" — passa i64::MAX
        // que o clamp(0, count) trata corretamente.
        let effective_end = if end == i64::MIN { i64::MAX } else { end };
        crate::namespaces::globals::string::rt::__RTS_FN_GL_STRING_SLICE(recv, start, effective_end)
    }
}

/// (cross-runtime #285) Runtime dispatch para `recv.includes(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_INCLUDES_AUTO(recv: u64, needle: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    let is_vec = with_entry(recv, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(recv, needle)
    } else {
        crate::namespaces::globals::string::rt::__RTS_FN_GL_STRING_INCLUDES(recv, needle as u64)
            as i64
    }
}

/// (cross-runtime #285) Runtime dispatch para `recv.indexOf(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_INDEX_OF_AUTO(recv: u64, needle: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    let is_vec = with_entry(recv, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF(recv, needle)
    } else {
        crate::namespaces::globals::string::rt::__RTS_FN_GL_STRING_INDEX_OF(recv, needle as u64)
    }
}

/// (cross-runtime #285) Runtime dispatch para `recv.lastIndexOf(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_LAST_INDEX_OF_AUTO(recv: u64, needle: i64) -> i64 {
    use super::super::gc::handles::with_entry;
    let is_vec = with_entry(recv, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF(recv, needle)
    } else {
        crate::namespaces::globals::string::rt::__RTS_FN_GL_STRING_LAST_INDEX_OF(
            recv,
            needle as u64,
        )
    }
}

/// (cross-runtime #52) `index in arr` — true se 0 <= index < length E
/// slot nao eh hole (sentinela i64::MIN+4). JS spec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_HAS_INDEX(handle: u64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_vec(handle, 0, |v| match v.get(index as usize).copied() {
        Some(slot) if slot != i64::MIN + 4 => 1,
        _ => 0,
    })
}

/// (#93) `typedArray.set(src, offset)`: copia os elementos de `src` (Vec)
/// para `dst` comecando em `offset`. Out-of-bounds eh ignorado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SET_FROM(dst: u64, src: u64, offset: i64) {
    let items: Vec<i64> = with_vec(src, Vec::new(), |v| v.clone());
    if offset < 0 {
        return;
    }
    with_vec_mut(dst, (), |v| {
        for (i, x) in items.into_iter().enumerate() {
            let idx = offset as usize + i;
            if let Some(slot) = v.get_mut(idx) {
                *slot = x;
            }
        }
    });
}

// (cross-runtime #257) Recursa em Vec aninhado arbitrariamente fundo.
// `top_sep` so' eh usado no nivel raiz; sub-arrays usam virgula fixa
// (JS spec: Array.prototype.toString chama join(",") recursivo).
fn join_into(out: &mut Vec<u8>, elems: &[i64], sep_bytes: &[u8], depth: u32) {
    // Guard contra ciclo. JS spec usa Set de visited; cap simples basta
    // pra nossos testes (depth normalmente <10).
    const MAX_DEPTH: u32 = 32;
    let inner_sep: &[u8] = if depth == 0 { sep_bytes } else { b"," };
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(inner_sep);
        }
        if *e == i64::MIN {
            out.extend_from_slice(b"false");
            continue;
        }
        if *e == i64::MIN + 1 {
            out.extend_from_slice(b"true");
            continue;
        }
        // (cross-runtime #142/#52) undefined/null/hole em join viram "".
        if *e == i64::MIN + 2 || *e == i64::MIN + 3 || *e == i64::MIN + 4 {
            continue;
        }
        let h = *e as u64;
        let as_str: Option<Vec<u8>> = with_entry(h, |entry| match entry {
            Some(Entry::String(b)) => Some(b.clone()),
            _ => None,
        });
        if let Some(b) = as_str {
            out.extend_from_slice(&b);
            continue;
        }
        let as_nested: Option<Vec<i64>> = with_entry(h, |entry| match entry {
            Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
            _ => None,
        });
        if let Some(sub) = as_nested {
            if depth < MAX_DEPTH {
                join_into(out, &sub, sep_bytes, depth + 1);
            }
            continue;
        }
        // (#1275) Slot float: armazenado como bits f64 (codegen). Heuristica:
        // valor fora do range de safe integer (>2^53) que decodifica como f64
        // finito eh um float. Formata via format_js_number (3.0 -> "3").
        const MAX_SAFE: i64 = (1i64 << 53) - 1;
        if *e > MAX_SAFE || *e < -MAX_SAFE {
            let f = f64::from_bits(*e as u64);
            if f.is_finite() && !f.is_nan() {
                out.extend_from_slice(
                    crate::namespaces::gc::string_pool::format_js_number(f).as_bytes(),
                );
                continue;
            }
        }
        out.extend_from_slice(e.to_string().as_bytes());
    }
}

// ─────────────────────────────────────────────────────────────────────────
// (#208 / #476) Array prototype methods sem callback. Aceitam args
// concretos — funcionam com qualquer expr de arg, nao so Ident de user fn.
// Implementacao espelha semantica JS basica.
// ─────────────────────────────────────────────────────────────────────────

/// (cross-runtime #285) Compara dois slots i64 considerando string content
/// quando ambos sao handles String. Sem isso, `["a","b"].indexOf("a")` falha
/// se "a" foi alocado em handles diferentes em compile time.
fn slot_eq(a: i64, b: i64) -> bool {
    if a == b {
        return true;
    }
    use super::super::gc::handles::with_entry;
    let a_str: Option<Vec<u8>> = with_entry(a as u64, |e| match e {
        Some(Entry::String(s)) => Some(s.clone()),
        _ => None,
    });
    if let Some(ab) = a_str {
        let b_eq = with_entry(b as u64, |e| match e {
            Some(Entry::String(bb)) => bb.as_slice() == ab.as_slice(),
            _ => false,
        });
        if b_eq {
            return true;
        }
    }
    false
}

/// `arr.indexOf(needle)` — primeiro index com `v == needle`, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF(handle: u64, needle: i64) -> i64 {
    // (cross-runtime #285) Snapshot do vec fora do lock para evitar deadlock
    // quando slot_eq -> with_entry(slot) cai no mesmo shard do vec.
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    snapshot
        .iter()
        .position(|x| slot_eq(*x, needle))
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// (#208) `arr.indexOf(needle, fromIndex)` — busca a partir de `from`.
/// Negativo = relativo ao fim. Fora do range = retorna -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = snapshot.len() as i64;
    let start = if from < 0 { (len + from).max(0) } else { from } as usize;
    if start >= snapshot.len() {
        return -1;
    }
    snapshot[start..]
        .iter()
        .position(|x| slot_eq(*x, needle))
        .map(|i| (i + start) as i64)
        .unwrap_or(-1)
}

/// `arr.lastIndexOf(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF(handle: u64, needle: i64) -> i64 {
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    snapshot
        .iter()
        .rposition(|x| slot_eq(*x, needle))
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// (#208) `arr.lastIndexOf(needle, fromIndex)` — busca de tras pra frente,
/// comecando em `from` (inclusive). Negativo = relativo ao fim.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = snapshot.len() as i64;
    let end = if from < 0 {
        (len + from).max(-1) + 1
    } else {
        (from + 1).min(len)
    } as usize;
    if end == 0 {
        return -1;
    }
    snapshot[..end]
        .iter()
        .rposition(|x| slot_eq(*x, needle))
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// `arr.includes(needle)` → 1 ou 0.
/// (cross-runtime #285) `slot_eq` compara conteudo de strings quando ambos
/// sao String handles — sem isso falha em closures que reusam literais.
/// Snapshot antes do iter pra evitar deadlock com with_entry nested.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(handle: u64, needle: i64) -> i64 {
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if snapshot.iter().any(|x| slot_eq(*x, needle)) {
        1
    } else {
        0
    }
}

/// (#208) `arr.includes(needle, fromIndex)` — busca a partir de `from`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    let snapshot: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = snapshot.len() as i64;
    let start = if from < 0 { (len + from).max(0) } else { from } as usize;
    if start >= snapshot.len() {
        return 0;
    }
    if snapshot[start..].iter().any(|x| slot_eq(*x, needle)) {
        1
    } else {
        0
    }
}

/// `arr.reverse()` in-place. Retorna o proprio handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_REVERSE(handle: u64) -> u64 {
    with_vec_mut(handle, (), |v| v.reverse());
    handle
}

/// `arr.shift()` — remove e retorna primeiro elemento. JS spec:
/// undefined se vazio. Mesmo padrao de POP (handle "undefined").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SHIFT(handle: u64) -> i64 {
    let first: Option<i64> = with_vec_mut(handle, None, |v| {
        if v.is_empty() {
            None
        } else {
            Some(v.remove(0))
        }
    });
    match first {
        Some(v) => v,
        // Vazio: undefined real (sentinel), nao handle-string.
        None => i64::MIN + 2,
    }
}

/// `arr.unshift(v)` — insere no começo. Retorna novo length.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT(handle: u64, value: i64) -> i64 {
    let limit_hit = with_vec_mut(handle, false, |v| {
        if v.len() >= VEC_MAX_LEN {
            return true;
        }
        v.insert(0, value);
        false
    });
    if limit_hit {
        eprintln!("RTS runtime: vec unshift exceeded limit of {VEC_MAX_LEN} elements; aborting");
        std::process::abort();
    }
    with_vec(handle, 0, |v| v.len() as i64)
}

/// `arr.slice(start, end)` — retorna handle de novo Vec com cópia do range.
/// `end < 0` significa relativo ao fim. `end == i64::MIN` = não fornecido (até o fim).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SLICE(handle: u64, start: i64, end: i64) -> u64 {
    let copy: Vec<i64> = with_vec(handle, Vec::new(), |v| {
        let len = v.len() as i64;
        let s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let e_eff = if end == i64::MIN {
            len
        } else if end < 0 {
            (len + end).max(0)
        } else {
            end.min(len)
        };
        let e = (e_eff as usize).max(s);
        v[s..e].to_vec()
    });
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// `arr.concat(other)` — retorna novo Vec com elementos de ambos.
/// `other` é handle de outro Vec; se invalido, copia só o original.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_CONCAT(handle: u64, other: u64) -> u64 {
    let mut out: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let other_elems: Vec<i64> = with_vec(other, Vec::new(), |v| v.clone());
    out.extend(other_elems);
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (cross-runtime #143) Append de um arg `arr.concat(arg, ...)`. Se `arg`
/// eh handle de Vec, faz extend; caso contrario push do escalar. JS spec:
/// `Array.prototype.concat` so' faz spread em arrays "concat-spreadable".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_CONCAT_APPEND(handle: u64, arg: i64) -> u64 {
    let arg_h = arg as u64;
    let as_vec: Option<Vec<i64>> = with_entry(arg_h, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
        _ => None,
    });
    if let Some(items) = as_vec {
        with_vec_mut(handle, (), |v| v.extend(items));
        return handle;
    }
    // (cross-runtime #378) Array-like object with [Symbol.isConcatSpreadable] =
    // true (truthy bool sentinel): spread its 0..length indexed elements, per
    // Array.prototype.concat. `{0:4,1:5,length:2,[Symbol.isConcatSpreadable]:true}`
    // → 4,5.
    let spread: Option<Vec<i64>> = with_entry(arg_h, |entry| match entry {
        Some(Entry::Map(m)) => {
            let spreadable = matches!(
                m.get("Symbol.isConcatSpreadable").copied(),
                Some(v) if v == i64::MIN + 1
            );
            if !spreadable {
                return None;
            }
            let len = m.get("length").copied().unwrap_or(0).max(0);
            let mut items = Vec::with_capacity(len as usize);
            for i in 0..len {
                items.push(m.get(&i.to_string()).copied().unwrap_or(0));
            }
            Some(items)
        }
        _ => None,
    });
    if let Some(items) = spread {
        with_vec_mut(handle, (), |v| v.extend(items));
    } else {
        with_vec_mut(handle, (), |v| v.push(arg));
    }
    handle
}

/// `arr.fill(value, start, end)` — preenche range com `value` in-place.
/// Retorna o proprio handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FILL(
    handle: u64,
    value: i64,
    start: i64,
    end: i64,
) -> u64 {
    with_vec_mut(handle, (), |v| {
        let len = v.len() as i64;
        let s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let e_eff = if end == i64::MIN {
            len
        } else if end < 0 {
            (len + end).max(0)
        } else {
            end.min(len)
        };
        let e = (e_eff as usize).max(s);
        for slot in &mut v[s..e] {
            *slot = value;
        }
    });
    handle
}

/// `arr.flat()` (depth=1) — concatena Vec<Vec<i64>> em Vec<i64>. Cada
/// elemento que for handle de Vec é expandido; outros são mantidos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FLAT(handle: u64) -> u64 {
    let elems: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let mut out: Vec<i64> = Vec::new();
    for e in elems {
        let h = e as u64;
        let inner: Option<Vec<i64>> = with_entry(h, |entry| match entry {
            Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
            _ => None,
        });
        if let Some(v) = inner {
            out.extend(v);
        } else {
            out.push(e);
        }
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `arr.flat(depth)` recursivo. depth < 0 trata como 0; depth grande
/// (> 1024) e' clamp pra 1024 (proxy de Infinity).
fn flat_recursive(elems: &[i64], depth: i64, out: &mut Vec<i64>) {
    for &e in elems {
        if depth <= 0 {
            out.push(e);
            continue;
        }
        let h = e as u64;
        let inner: Option<Vec<i64>> = with_entry(h, |entry| match entry {
            Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
            _ => None,
        });
        if let Some(v) = inner {
            flat_recursive(&v, depth - 1, out);
        } else {
            out.push(e);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FLAT_DEPTH(handle: u64, depth: i64) -> u64 {
    let elems: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let depth = depth.clamp(0, 1024);
    let mut out: Vec<i64> = Vec::new();
    flat_recursive(&elems, depth, &mut out);
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `arr.splice(start, deleteCount)` — remove `deleteCount` elementos a
/// partir de `start` in-place. Retorna handle de Vec com os removidos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE(
    handle: u64,
    start: i64,
    delete_count: i64,
) -> u64 {
    let removed: Vec<i64> = with_vec_mut(handle, Vec::new(), |v| {
        let len = v.len() as i64;
        let s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let count = delete_count.max(0).min(len - s as i64) as usize;
        v.drain(s..s + count).collect()
    });
    alloc_entry(Entry::Vec(Box::new(removed)))
}

/// `arr.splice(start, deleteCount, ...items)` — remove `deleteCount` elementos
/// e insere `items_handle` (Vec) na mesma posicao. Retorna Vec dos removidos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SPLICE_INSERT(
    handle: u64,
    start: i64,
    delete_count: i64,
    items_handle: u64,
) -> u64 {
    let items: Vec<i64> = with_entry(items_handle, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let removed: Vec<i64> = with_vec_mut(handle, Vec::new(), |v| {
        let len = v.len() as i64;
        let s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let count = delete_count.max(0).min(len - s as i64) as usize;
        let drained: Vec<i64> = v.drain(s..s + count).collect();
        for (i, item) in items.into_iter().enumerate() {
            v.insert(s + i, item);
        }
        drained
    });
    alloc_entry(Entry::Vec(Box::new(removed)))
}

/// (#780/#786) `new Array(len)` — cria Vec preenchido com sentinel de HOLE
/// `i64::MIN + 4` (cross-runtime #1533): JS spec cria slots vazios (holes),
/// nao undefined — `0 in new Array(3)` eh false. Leitura `a[0] === undefined`
/// continua true (codegen trata MIN+4 como undefined-equivalente). Limitado
/// ao `VEC_MAX_LEN` pra evitar OOM.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_NEW_WITH_LENGTH(len: i64) -> u64 {
    let len = len.max(0).min(VEC_MAX_LEN as i64) as usize;
    let v = vec![i64::MIN + 4; len];
    alloc_entry(Entry::Vec(Box::new(v)))
}

/// (#208) `Array.from({length: n}, fn?)` — gera Vec [fn(0), fn(1), ...].
/// Se fn_ptr == 0, gera [0, 1, ..., n-1] (sem mapeamento).
/// fn_ptr e' `extern "C" fn(item: i64, idx: i64) -> i64`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM_LENGTH(n: i64, fn_ptr: u64) -> u64 {
    if n < 0 || n > VEC_MAX_LEN as i64 {
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

/// (#208) `Array.from(vecHandle, fn?)` — converte Vec existente, opcionalmente
/// mapeando cada elemento. Sem fn, retorna copia rasa.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM_VEC(src: u64, fn_ptr: u64) -> u64 {
    // (#477) Generator lazy (state-machine): Array.from(gen()) drena o
    // generator ate `done` num Vec (igual o for-of em map.rs). Sem isto,
    // GenState nao casava Vec/Map/Buffer e Array.from devolvia [] (fib= vazio
    // no 108_generator_functions). Generator finito -> Vec; infinito travaria
    // igual a JS.
    let src = {
        use crate::namespaces::gc::handles::{with_entry, Entry};
        let is_sm = with_entry(src, |e| matches!(e, Some(Entry::GenState(_))));
        if is_sm {
            crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_DRAIN(src)
        } else {
            src
        }
    };
    // Aceita Entry::Vec OU Entry::Map (Set/Map). Para Set (backing eh
    // Map<key,1>), itera keys parseando como i64. Para Map normal,
    // itera values.
    let is_set = crate::namespaces::collections::map::handle_is_set_kind(src);
    let is_map = crate::namespaces::collections::map::handle_is_map_kind(src);
    let src_items: Vec<i64> = with_entry(src, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        // (cross-runtime #58) Array.from(Uint8Array)/Buffer: cada byte
        // vira slot i64.
        Some(Entry::Buffer(b)) => b.iter().map(|&x| x as i64).collect(),
        Some(Entry::Map(m)) => {
            if is_set {
                // (#394) Recupera identidade do elemento via value preservado
                // (set_element_from_pair) em vez de descartar nao-numericos.
                m.iter()
                    .map(|(k, &v)| crate::namespaces::collections::map::set_element_from_pair(k, v))
                    .collect()
            } else if is_map {
                // JS Map: Array.from(map) retorna pares [key, value].
                m.iter()
                    .map(|(k, &v)| {
                        let key_h = alloc_entry(Entry::String(k.as_bytes().to_vec())) as i64;
                        let pair = alloc_entry(Entry::Vec(Box::new(vec![key_h, v]))) as i64;
                        pair
                    })
                    .collect()
            } else {
                m.values().copied().collect()
            }
        }
        _ => Vec::new(),
    });
    if fn_ptr == 0 {
        return alloc_entry(Entry::Vec(Box::new(src_items)));
    }
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut out: Vec<i64> = Vec::with_capacity(src_items.len());
    for (i, item) in src_items.into_iter().enumerate() {
        out.push(f(item, i as i64));
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#208) `arr.findLast(fn)` — ultimo elemento que satisfaz, ou undefined.
/// (cross-runtime #260) Miss retorna sentinela MIN+2 (undefined); codegen
/// marca como Handle pra TPL_COERCE_AUTO resolver via tabela de sentinelas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST(handle: u64, fn_ptr: u64) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        return i64::MIN + 2;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    items
        .into_iter()
        .rev()
        .find(|&x| f(x) != 0)
        .unwrap_or(i64::MIN + 2)
}

/// (#208) `arr.findLastIndex(fn)` — index do ultimo match, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX(handle: u64, fn_ptr: u64) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        return -1;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let len = items.len();
    for (i, x) in items.into_iter().enumerate().rev() {
        if f(x) != 0 {
            return i as i64;
        }
    }
    let _ = len;
    -1
}

/// (#208) `arr.reduceRight(fn, init)` — reduce da direita pra esquerda.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT(
    handle: u64,
    init: i64,
    fn_ptr: u64,
) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        return init;
    }
    // (#345 follow-up) callback liftado de reduce tem 3 params (acc, val,
    // index) — o lift do parallelism gera `__lifted_arr_method_reduce_N` com
    // aridade 3. Sem passar o index como 3o arg, o param `i` lia lixo do
    // registrador (`arr.reduceRight((a,x,i)=>a+x+i,0)` -> resultado errado).
    // reduceRight visita da DIREITA p/ esquerda, mas o index eh a POSICAO
    // ORIGINAL do elemento (JS spec): len-1, len-2, ..., 0.
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = init;
    for (i, x) in items.into_iter().enumerate().rev() {
        acc = f(acc, x, i as i64);
    }
    acc
}

/// (cross-runtime #202) `arr.reduceRight(fn)` sem initial value. JS spec:
/// ultimo elemento vira acc, loop comeca do penultimo indo pra esquerda.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT_NO_INIT(
    handle: u64,
    fn_ptr: u64,
) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 || items.is_empty() {
        return 0;
    }
    // (#345 follow-up) idem REDUCE_RIGHT: callback de 3 params (acc, val,
    // index). Sem init, o ultimo elemento (items[len-1]) vira acc inicial e o
    // loop comeca em len-2 descendo ate 0 (index = posicao original).
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let last = items.len() - 1;
    let mut acc = items[last];
    for i in (0..last).rev() {
        acc = f(acc, items[i], i as i64);
    }
    acc
}

/// (#208) `arr.flatMap(fn)` — map + flat depth=1.
/// Cada elemento de `fn(x)` deve ser handle de Vec; senao adiciona o
/// proprio valor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP(handle: u64, fn_ptr: u64) -> u64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        return alloc_entry(Entry::Vec(Box::new(items)));
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut out: Vec<i64> = Vec::new();
    for x in items {
        let mapped = f(x);
        let h = mapped as u64;
        let inner: Option<Vec<i64>> = with_entry(h, |e| match e {
            Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
            _ => None,
        });
        match inner {
            Some(v) => out.extend(v),
            None => out.push(mapped),
        }
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#208) `arr.copyWithin(target, start?, end?)` — copia secao do array
/// pra outra posicao no mesmo array, in-place. Retorna o proprio handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN(
    handle: u64,
    target: i64,
    start: i64,
    end: i64,
) -> u64 {
    with_vec_mut(handle, (), |v| {
        let len = v.len() as i64;
        let to = if target < 0 {
            (len + target).max(0)
        } else {
            target.min(len)
        } as usize;
        let from_s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let from_e_eff = if end == i64::MIN {
            len
        } else if end < 0 {
            (len + end).max(0)
        } else {
            end.min(len)
        };
        let from_e = (from_e_eff as usize).max(from_s);
        let count = (from_e - from_s).min(v.len() - to);
        // Snapshot pra evitar aliasing in-place quando ranges se sobrepoem.
        let snapshot: Vec<i64> = v[from_s..from_s + count].to_vec();
        for (i, val) in snapshot.into_iter().enumerate() {
            v[to + i] = val;
        }
    });
    handle
}

/// (#208) `arr.sort()` — sem comparator: ordem JS lexicografica de Number/string
/// implementada como sort numerico ascendente em i64.
/// `arr.sort(fn)` com comparator — fn(a,b) deve retornar negativo/0/positivo.
/// Retorna o proprio handle (in-place).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SORT(handle: u64, fn_ptr: u64) -> u64 {
    if fn_ptr == 0 {
        // Detecta se elementos sao string handles; se sim, sort lexico-
        // grafico. Senao, sort numerico i64 default.
        let all_strings = with_vec(handle, false, |v| {
            !v.is_empty()
                && v.iter()
                    .all(|x| with_entry(*x as u64, |e| matches!(e, Some(Entry::String(_)))))
        });
        if all_strings {
            // Snapshot bytes pra evitar nested locks.
            let pairs: Vec<(i64, Vec<u8>)> = with_vec(handle, Vec::new(), |v| {
                v.iter()
                    .map(|x| {
                        let bytes: Vec<u8> = with_entry(*x as u64, |e| match e {
                            Some(Entry::String(b)) => b.clone(),
                            _ => Vec::new(),
                        });
                        (*x, bytes)
                    })
                    .collect()
            });
            let mut sorted = pairs;
            sorted.sort_by(|a, b| a.1.cmp(&b.1));
            with_vec_mut(handle, (), |v| {
                v.clear();
                for (h, _) in sorted {
                    v.push(h);
                }
            });
        } else {
            with_vec_mut(handle, (), |v| v.sort());
        }
    } else {
        let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        with_vec_mut(handle, (), |v| {
            v.sort_by(|a, b| {
                let r = f(*a, *b);
                if r < 0 {
                    std::cmp::Ordering::Less
                } else if r > 0 {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            });
        });
    }
    handle
}

/// (#208) `arr.values()` — Vec eager com copia dos valores.
/// Em JS spec retorna Array Iterator; v0 RTS retorna Vec direto
/// (compativel com `for-of` e `.join()`). Iterator real exige
/// Symbol.iterator (PR separada).
/// (#61) Tambem suporta Map/Set handle: retorna values do Map. Set em
/// RTS eh Map onde key==value (semantica de `new Set([x,y])`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_VALUES(handle: u64) -> u64 {
    use super::super::gc::handles::with_entry;
    // (#61) Set armazena value=1 marker no Map<String,i64>; values reais
    // sao as keys. Detecta via handle_is_set_kind antes do match Map.
    let is_set = crate::namespaces::collections::map::handle_is_set_kind(handle);
    enum Kind {
        Vec(Vec<i64>),
        MapVals(Vec<i64>),
        SetPairs(Vec<(String, i64)>),
        Other,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => Kind::Vec((**v).clone()),
        Some(Entry::Map(m)) => {
            if is_set {
                Kind::SetPairs(m.iter().map(|(k, &v)| (k.clone(), v)).collect())
            } else {
                Kind::MapVals(m.values().copied().collect())
            }
        }
        _ => Kind::Other,
    });
    let out: Vec<i64> = match kind {
        Kind::Vec(v) | Kind::MapVals(v) => v,
        // (#394) Recupera identidade do elemento via value preservado.
        Kind::SetPairs(pairs) => pairs
            .into_iter()
            .map(|(k, v)| crate::namespaces::collections::map::set_element_from_pair(&k, v))
            .collect(),
        Kind::Other => Vec::new(),
    };
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#208) `arr.keys()` — Vec [0, 1, ..., len-1].
/// (#61) Tambem suporta Map: retorna chaves como string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_KEYS(handle: u64) -> u64 {
    use super::super::gc::handles::with_entry;
    enum Kind {
        Vec(usize),
        Map(Vec<String>),
        Other,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => Kind::Vec(v.len()),
        Some(Entry::Map(m)) => Kind::Map(m.keys().cloned().collect()),
        _ => Kind::Other,
    });
    let out: Vec<i64> = match kind {
        Kind::Vec(len) => (0..len as i64).collect(),
        Kind::Map(keys) => keys
            .into_iter()
            .map(|k| alloc_entry(Entry::String(k.into_bytes())) as i64)
            .collect(),
        Kind::Other => Vec::new(),
    };
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#208 ES2023) `arr.toSorted()` — sort imutavel: clona, ordena, retorna novo handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED(handle: u64, fn_ptr: u64) -> u64 {
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        // Detecta string handles e ordena lexico (mesma logica do sort).
        let all_strings = !copy.is_empty()
            && copy
                .iter()
                .all(|x| with_entry(*x as u64, |e| matches!(e, Some(Entry::String(_)))));
        if all_strings {
            let pairs: Vec<(i64, Vec<u8>)> = copy
                .iter()
                .map(|x| {
                    let bytes: Vec<u8> = with_entry(*x as u64, |e| match e {
                        Some(Entry::String(b)) => b.clone(),
                        _ => Vec::new(),
                    });
                    (*x, bytes)
                })
                .collect();
            let mut sorted = pairs;
            sorted.sort_by(|a, b| a.1.cmp(&b.1));
            copy = sorted.into_iter().map(|(h, _)| h).collect();
        } else {
            copy.sort();
        }
    } else {
        let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        copy.sort_by(|a, b| {
            let r = f(*a, *b);
            if r < 0 {
                std::cmp::Ordering::Less
            } else if r > 0 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
    }
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208 ES2023) `arr.toReversed()` — reverse imutavel.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED(handle: u64) -> u64 {
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    copy.reverse();
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208 ES2023) `arr.toSpliced(start, deleteCount)` — splice imutavel.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED(
    handle: u64,
    start: i64,
    delete_count: i64,
) -> u64 {
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = copy.len() as i64;
    let s = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    } as usize;
    let count = delete_count.max(0).min(len - s as i64) as usize;
    copy.drain(s..s + count);
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// `arr.toSpliced(start, deleteCount, ...items)` com inserts.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_INSERT(
    handle: u64,
    start: i64,
    delete_count: i64,
    items_handle: u64,
) -> u64 {
    let items: Vec<i64> = with_entry(items_handle, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = copy.len() as i64;
    let s = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    } as usize;
    let count = delete_count.max(0).min(len - s as i64) as usize;
    copy.drain(s..s + count);
    for (i, item) in items.into_iter().enumerate() {
        copy.insert(s + i, item);
    }
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208 ES2023) `arr.with(idx, value)` — substitui elemento, retorna novo Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_WITH(handle: u64, idx: i64, value: i64) -> u64 {
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let len = copy.len() as i64;
    let i = if idx < 0 {
        (len + idx).max(0)
    } else {
        idx.min(len - 1)
    } as usize;
    if i < copy.len() {
        copy[i] = value;
    }
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208) `arr.entries()` — Vec de Vec[[idx, value], ...].
/// Cada entry e' um Vec<i64> com 2 elementos.
/// (#61) Tambem suporta Map handle: entries de [key_str_handle, value].
/// Quando codegen nao sabe se o receiver eh Map ou Vec (member access em
/// var ambigua), este path generico funciona pra ambos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_ENTRIES(handle: u64) -> u64 {
    use super::super::gc::handles::with_entry;
    enum Kind {
        Vec(Vec<i64>),
        Map(Vec<(String, i64)>),
        Other,
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => Kind::Vec((**v).clone()),
        Some(Entry::Map(m)) => Kind::Map(m.iter().map(|(k, v)| (k.clone(), *v)).collect()),
        _ => Kind::Other,
    });
    let mut out: Vec<i64> = Vec::new();
    match kind {
        Kind::Vec(items) => {
            out.reserve(items.len());
            for (i, val) in items.into_iter().enumerate() {
                let pair = alloc_entry(Entry::Vec(Box::new(vec![i as i64, val])));
                out.push(pair as i64);
            }
        }
        Kind::Map(items) => {
            out.reserve(items.len());
            for (k, v) in items {
                let key_h = alloc_entry(Entry::String(k.into_bytes())) as i64;
                let pair = alloc_entry(Entry::Vec(Box::new(vec![key_h, v])));
                out.push(pair as i64);
            }
        }
        Kind::Other => {}
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle_to_vec(h: u64) -> Vec<i64> {
        with_vec(h, Vec::new(), |v| v.clone())
    }

    #[test]
    fn index_of_finds_first_match() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [10, 20, 30, 20] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF(h, 20), 1);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF(h, 20), 3);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF(h, 99), -1);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(h, 30), 1);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(h, 99), 0);
    }

    #[test]
    fn reverse_in_place() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_REVERSE(h), h);
        assert_eq!(handle_to_vec(h), vec![3, 2, 1]);
    }

    #[test]
    fn shift_unshift_roundtrip() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [2, 3] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT(h, 1), 3);
        assert_eq!(handle_to_vec(h), vec![1, 2, 3]);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_SHIFT(h), 1);
        assert_eq!(handle_to_vec(h), vec![2, 3]);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_SHIFT(h), 2);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_SHIFT(h), 3);
        // Vazio: retorna o sentinel undefined (i64::MIN+2), nao handle-string,
        // p/ que `?? -1` e `=== undefined` funcionem no codegen.
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_SHIFT(h), i64::MIN + 2);
    }

    #[test]
    fn slice_positive_and_negative() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [10, 20, 30, 40, 50] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let s1 = __RTS_FN_NS_COLLECTIONS_VEC_SLICE(h, 1, 3);
        assert_eq!(handle_to_vec(s1), vec![20, 30]);
        let s2 = __RTS_FN_NS_COLLECTIONS_VEC_SLICE(h, -2, i64::MIN);
        assert_eq!(handle_to_vec(s2), vec![40, 50]);
        let s3 = __RTS_FN_NS_COLLECTIONS_VEC_SLICE(h, 0, -1);
        assert_eq!(handle_to_vec(s3), vec![10, 20, 30, 40]);
    }

    #[test]
    fn concat_appends_other() {
        let a = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(a, x);
        }
        let b = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [3, 4] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(b, x);
        }
        let c = __RTS_FN_NS_COLLECTIONS_VEC_CONCAT(a, b);
        assert_eq!(handle_to_vec(c), vec![1, 2, 3, 4]);
    }

    #[test]
    fn fill_partial_range() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3, 4, 5] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        __RTS_FN_NS_COLLECTIONS_VEC_FILL(h, 0, 1, 4);
        assert_eq!(handle_to_vec(h), vec![1, 0, 0, 0, 5]);
    }

    #[test]
    fn flat_one_level() {
        let inner1 = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(inner1, x);
        }
        let inner2 = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [3, 4] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(inner2, x);
        }
        let outer = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        __RTS_FN_NS_COLLECTIONS_VEC_PUSH(outer, inner1 as i64);
        __RTS_FN_NS_COLLECTIONS_VEC_PUSH(outer, inner2 as i64);
        let flat = __RTS_FN_NS_COLLECTIONS_VEC_FLAT(outer);
        assert_eq!(handle_to_vec(flat), vec![1, 2, 3, 4]);
    }

    #[test]
    fn splice_remove_returns_extracted() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3, 4, 5] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let removed = __RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE(h, 1, 2);
        assert_eq!(handle_to_vec(removed), vec![2, 3]);
        assert_eq!(handle_to_vec(h), vec![1, 4, 5]);
    }

    #[test]
    fn array_from_length_no_fn() {
        let h = __RTS_FN_GL_ARRAY_FROM_LENGTH(4, 0);
        assert_eq!(handle_to_vec(h), vec![0, 1, 2, 3]);
    }

    extern "C" fn triple(_item: i64, idx: i64) -> i64 {
        idx * 3
    }

    #[test]
    fn array_from_length_with_fn() {
        let fp = triple as *const () as u64;
        let h = __RTS_FN_GL_ARRAY_FROM_LENGTH(4, fp);
        assert_eq!(handle_to_vec(h), vec![0, 3, 6, 9]);
    }

    #[test]
    fn array_from_vec_no_fn() {
        let src = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [10, 20, 30] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(src, x);
        }
        let cp = __RTS_FN_GL_ARRAY_FROM_VEC(src, 0);
        assert_eq!(handle_to_vec(cp), vec![10, 20, 30]);
        assert_ne!(cp, src);
    }

    #[test]
    fn sort_default_ascending() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [3, 1, 4, 1, 5, 9, 2, 6] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        __RTS_FN_NS_COLLECTIONS_VEC_SORT(h, 0);
        assert_eq!(handle_to_vec(h), vec![1, 1, 2, 3, 4, 5, 6, 9]);
    }

    extern "C" fn cmp_desc(a: i64, b: i64) -> i64 {
        b - a
    }

    #[test]
    fn sort_with_comparator() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 5, 3] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let fp = cmp_desc as *const () as u64;
        __RTS_FN_NS_COLLECTIONS_VEC_SORT(h, fp);
        assert_eq!(handle_to_vec(h), vec![5, 3, 1]);
    }

    extern "C" fn gt_two(x: i64) -> i64 {
        if x > 2 {
            1
        } else {
            0
        }
    }

    #[test]
    fn find_last_finds_from_end() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 3, 5, 2, 4] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let fp = gt_two as *const () as u64;
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST(h, fp), 4);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX(h, fp), 4);
    }

    extern "C" fn add(a: i64, b: i64) -> i64 {
        a + b
    }

    #[test]
    fn reduce_right_works() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let fp = add as *const () as u64;
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT(h, 0, fp), 6);
    }

    extern "C" fn double_as_pair(x: i64) -> i64 {
        let inner = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        __RTS_FN_NS_COLLECTIONS_VEC_PUSH(inner, x);
        __RTS_FN_NS_COLLECTIONS_VEC_PUSH(inner, x * 2);
        inner as i64
    }

    #[test]
    fn flat_map_expands_pairs() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let fp = double_as_pair as *const () as u64;
        let r = __RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP(h, fp);
        assert_eq!(handle_to_vec(r), vec![1, 2, 2, 4, 3, 6]);
    }

    #[test]
    fn copy_within_basic() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [1, 2, 3, 4, 5] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        // copyWithin(0, 3) → copia elementos a partir do idx 3 pro idx 0
        __RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN(h, 0, 3, i64::MIN);
        assert_eq!(handle_to_vec(h), vec![4, 5, 3, 4, 5]);
    }

    #[test]
    fn values_returns_copy() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [10, 20, 30] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let cp = __RTS_FN_NS_COLLECTIONS_VEC_VALUES(h);
        assert_eq!(handle_to_vec(cp), vec![10, 20, 30]);
        assert_ne!(cp, h);
    }

    #[test]
    fn keys_returns_indices() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [10, 20, 30, 40] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let k = __RTS_FN_NS_COLLECTIONS_VEC_KEYS(h);
        assert_eq!(handle_to_vec(k), vec![0, 1, 2, 3]);
    }

    #[test]
    fn entries_returns_pairs() {
        let h = __RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for x in [100, 200] {
            __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, x);
        }
        let entries = __RTS_FN_NS_COLLECTIONS_VEC_ENTRIES(h);
        let outer = handle_to_vec(entries);
        assert_eq!(outer.len(), 2);
        assert_eq!(handle_to_vec(outer[0] as u64), vec![0, 100]);
        assert_eq!(handle_to_vec(outer[1] as u64), vec![1, 200]);
    }
}
