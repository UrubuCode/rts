//! Vec<i64> — lista ordenada de valores i64.

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};

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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64 {
    alloc_entry(Entry::Vec(Box::new(Vec::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FREE(handle: u64) {
    free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LEN(handle: u64) -> i64 {
    with_vec(handle, -1, |v| v.len() as i64)
}

/// Limite duro de elementos por vec — protege contra OOM em cenarios
/// patologicos (ex: generator infinito desugared para buffer eager,
/// loop sem condicao de parada). 1M i64 = 8MiB por vec, suficiente
/// pro caso real e barato comparado aos GBs que um leak descontrolado
/// produz.
const VEC_MAX_LEN: usize = 1_000_000;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64) {
    let limit_hit = with_vec_mut(handle, false, |v| {
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

/// Remove e retorna o ultimo valor, ou 0 se vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_POP(handle: u64) -> i64 {
    with_vec_mut(handle, 0, |v| v.pop().unwrap_or(0))
}

/// Valor em `index`, ou 0 fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_GET(handle: u64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_vec(handle, 0, |v| v.get(index as usize).copied().unwrap_or(0))
}

/// Escreve `value` em `index`. No-op fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SET(handle: u64, index: i64, value: i64) {
    if index < 0 {
        return;
    }
    with_vec_mut(handle, (), |v| {
        if let Some(slot) = v.get_mut(index as usize) {
            *slot = value;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_CLEAR(handle: u64) {
    with_vec_mut(handle, (), |v| v.clear());
}

/// Junta os elementos do Vec interpretando cada i64 como:
///   - string handle valido → conteudo da string
///   - caso contrario → representacao decimal do numero
/// Retorna handle de string nova com os elementos separados por `sep_h`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_JOIN(handle: u64, sep_h: u64) -> u64 {
    // Snapshot dos elementos sem segurar o lock — formatar pode tocar
    // outros shards (resolver string handles).
    let elems: Option<Vec<i64>> = with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
        _ => None,
    });
    let Some(elems) = elems else { return 0; };

    // Resolve separador como bytes; vazio se handle invalido.
    let sep_bytes: Vec<u8> = with_entry(sep_h, |entry| match entry {
        Some(Entry::String(b)) => b.clone(),
        _ => Vec::new(),
    });

    let mut out: Vec<u8> = Vec::new();
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(&sep_bytes);
        }
        // Sentinela bool em slot (codegen de array literal/Array.of).
        if *e == i64::MIN {
            out.extend_from_slice(b"false");
            continue;
        }
        if *e == i64::MIN + 1 {
            out.extend_from_slice(b"true");
            continue;
        }
        let h = *e as u64;
        // Tenta como string handle primeiro.
        let as_str: Option<Vec<u8>> = with_entry(h, |entry| match entry {
            Some(Entry::String(b)) => Some(b.clone()),
            _ => None,
        });
        if let Some(b) = as_str {
            out.extend_from_slice(&b);
            continue;
        }
        // (#476 follow-up) Vec aninhado: JS \`Array.toString()\` chama
        // \`.join(\",\")\` recursivo. Usa virgula fixa (spec) em sub-arrays.
        let as_nested: Option<Vec<i64>> = with_entry(h, |entry| match entry {
            Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
            _ => None,
        });
        if let Some(sub) = as_nested {
            for (j, sub_e) in sub.iter().enumerate() {
                if j > 0 { out.push(b','); }
                let sh = *sub_e as u64;
                let sub_str: Option<Vec<u8>> = with_entry(sh, |en| match en {
                    Some(Entry::String(b)) => Some(b.clone()),
                    _ => None,
                });
                if let Some(b) = sub_str {
                    out.extend_from_slice(&b);
                } else {
                    // Vec mais profundo ou primitivo. Formata recursivo.
                    let deeper: Option<Vec<i64>> = with_entry(sh, |en| match en {
                        Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
                        _ => None,
                    });
                    if let Some(d) = deeper {
                        for (k, d_e) in d.iter().enumerate() {
                            if k > 0 { out.push(b','); }
                            out.extend_from_slice(d_e.to_string().as_bytes());
                        }
                    } else {
                        out.extend_from_slice(sub_e.to_string().as_bytes());
                    }
                }
            }
            continue;
        }
        // Fallback: formata como i64 decimal.
        out.extend_from_slice(e.to_string().as_bytes());
    }

    alloc_entry(Entry::String(out))
}

// ─────────────────────────────────────────────────────────────────────────
// (#208 / #476) Array prototype methods sem callback. Aceitam args
// concretos — funcionam com qualquer expr de arg, nao so Ident de user fn.
// Implementacao espelha semantica JS basica.
// ─────────────────────────────────────────────────────────────────────────

/// `arr.indexOf(needle)` — primeiro index com `v == needle`, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF(handle: u64, needle: i64) -> i64 {
    with_vec(handle, -1, |v| {
        v.iter().position(|x| *x == needle).map(|i| i as i64).unwrap_or(-1)
    })
}

/// (#208) `arr.indexOf(needle, fromIndex)` — busca a partir de `from`.
/// Negativo = relativo ao fim. Fora do range = retorna -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    with_vec(handle, -1, |v| {
        let len = v.len() as i64;
        let start = if from < 0 { (len + from).max(0) } else { from } as usize;
        if start >= v.len() {
            return -1;
        }
        v[start..]
            .iter()
            .position(|x| *x == needle)
            .map(|i| (i + start) as i64)
            .unwrap_or(-1)
    })
}

/// `arr.lastIndexOf(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF(handle: u64, needle: i64) -> i64 {
    with_vec(handle, -1, |v| {
        v.iter().rposition(|x| *x == needle).map(|i| i as i64).unwrap_or(-1)
    })
}

/// (#208) `arr.lastIndexOf(needle, fromIndex)` — busca de tras pra frente,
/// comecando em `from` (inclusive). Negativo = relativo ao fim.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    with_vec(handle, -1, |v| {
        let len = v.len() as i64;
        let end = if from < 0 { (len + from).max(-1) + 1 } else { (from + 1).min(len) } as usize;
        if end == 0 {
            return -1;
        }
        v[..end]
            .iter()
            .rposition(|x| *x == needle)
            .map(|i| i as i64)
            .unwrap_or(-1)
    })
}

/// `arr.includes(needle)` → 1 ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(handle: u64, needle: i64) -> i64 {
    with_vec(handle, 0, |v| if v.contains(&needle) { 1 } else { 0 })
}

/// (#208) `arr.includes(needle, fromIndex)` — busca a partir de `from`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM(
    handle: u64,
    needle: i64,
    from: i64,
) -> i64 {
    with_vec(handle, 0, |v| {
        let len = v.len() as i64;
        let start = if from < 0 { (len + from).max(0) } else { from } as usize;
        if start >= v.len() {
            return 0;
        }
        if v[start..].contains(&needle) { 1 } else { 0 }
    })
}

/// `arr.reverse()` in-place. Retorna o proprio handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_REVERSE(handle: u64) -> u64 {
    with_vec_mut(handle, (), |v| v.reverse());
    handle
}

/// `arr.shift()` — remove e retorna primeiro elemento, ou 0 se vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SHIFT(handle: u64) -> i64 {
    with_vec_mut(handle, 0, |v| if v.is_empty() { 0 } else { v.remove(0) })
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
        let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
        let e_eff = if end == i64::MIN { len } else if end < 0 { (len + end).max(0) } else { end.min(len) };
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
        let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
        let e_eff = if end == i64::MIN { len } else if end < 0 { (len + end).max(0) } else { end.min(len) };
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
        let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
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
        let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
        let count = delete_count.max(0).min(len - s as i64) as usize;
        let drained: Vec<i64> = v.drain(s..s + count).collect();
        for (i, item) in items.into_iter().enumerate() {
            v.insert(s + i, item);
        }
        drained
    });
    alloc_entry(Entry::Vec(Box::new(removed)))
}

/// (#780) `new Array(len)` — cria Vec preenchido com 0 (undefined em V0)
/// Limitado ao `VEC_MAX_LEN` pra evitar OOM.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_NEW_WITH_LENGTH(len: i64) -> u64 {
    let len = len.max(0).min(VEC_MAX_LEN as i64) as usize;
    let v = vec![0; len];
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
        let f: extern "C" fn(i64, i64) -> i64 =
            unsafe { std::mem::transmute(fn_ptr as usize) };
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
    let src_items: Vec<i64> = with_vec(src, Vec::new(), |v| v.clone());
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

/// (#208) `arr.findLast(fn)` — ultimo elemento que satisfaz, ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST(handle: u64, fn_ptr: u64) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 { return 0; }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    items.into_iter().rev().find(|&x| f(x) != 0).unwrap_or(0)
}

/// (#208) `arr.findLastIndex(fn)` — index do ultimo match, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX(handle: u64, fn_ptr: u64) -> i64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 { return -1; }
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
    if fn_ptr == 0 { return init; }
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    items.into_iter().rev().fold(init, |a, b| f(a, b))
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
        let to = if target < 0 { (len + target).max(0) } else { target.min(len) } as usize;
        let from_s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
        let from_e_eff = if end == i64::MIN { len }
            else if end < 0 { (len + end).max(0) }
            else { end.min(len) };
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
            !v.is_empty() && v.iter().all(|x| {
                with_entry(*x as u64, |e| matches!(e, Some(Entry::String(_))))
            })
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
                if r < 0 { std::cmp::Ordering::Less }
                else if r > 0 { std::cmp::Ordering::Greater }
                else { std::cmp::Ordering::Equal }
            });
        });
    }
    handle
}

/// (#208) `arr.values()` — Vec eager com copia dos valores.
/// Em JS spec retorna Array Iterator; v0 RTS retorna Vec direto
/// (compativel com `for-of` e `.join()`). Iterator real exige
/// Symbol.iterator (PR separada).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_VALUES(handle: u64) -> u64 {
    let copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208) `arr.keys()` — Vec [0, 1, ..., len-1].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_KEYS(handle: u64) -> u64 {
    let len = with_vec(handle, 0, |v| v.len());
    let keys: Vec<i64> = (0..len as i64).collect();
    alloc_entry(Entry::Vec(Box::new(keys)))
}

/// (#208 ES2023) `arr.toSorted()` — sort imutavel: clona, ordena, retorna novo handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED(handle: u64, fn_ptr: u64) -> u64 {
    let mut copy: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    if fn_ptr == 0 {
        // Detecta string handles e ordena lexico (mesma logica do sort).
        let all_strings = !copy.is_empty()
            && copy.iter().all(|x| {
                with_entry(*x as u64, |e| matches!(e, Some(Entry::String(_))))
            });
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
            if r < 0 { std::cmp::Ordering::Less }
            else if r > 0 { std::cmp::Ordering::Greater }
            else { std::cmp::Ordering::Equal }
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
    let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
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
    let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
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
    let i = if idx < 0 { (len + idx).max(0) } else { idx.min(len - 1) } as usize;
    if i < copy.len() {
        copy[i] = value;
    }
    alloc_entry(Entry::Vec(Box::new(copy)))
}

/// (#208) `arr.entries()` — Vec de Vec[[idx, value], ...].
/// Cada entry e' um Vec<i64> com 2 elementos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_ENTRIES(handle: u64) -> u64 {
    let items: Vec<i64> = with_vec(handle, Vec::new(), |v| v.clone());
    let mut out: Vec<i64> = Vec::with_capacity(items.len());
    for (i, val) in items.into_iter().enumerate() {
        let pair = alloc_entry(Entry::Vec(Box::new(vec![i as i64, val])));
        out.push(pair as i64);
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
        assert_eq!(__RTS_FN_NS_COLLECTIONS_VEC_SHIFT(h), 0); // vazio
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
        if x > 2 { 1 } else { 0 }
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
