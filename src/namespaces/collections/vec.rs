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
        let h = *e as u64;
        // Tenta como string handle primeiro.
        let as_str: Option<Vec<u8>> = with_entry(h, |entry| match entry {
            Some(Entry::String(b)) => Some(b.clone()),
            _ => None,
        });
        if let Some(b) = as_str {
            out.extend_from_slice(&b);
        } else {
            // Fallback: formata como i64 decimal.
            out.extend_from_slice(e.to_string().as_bytes());
        }
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

/// `arr.lastIndexOf(needle)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF(handle: u64, needle: i64) -> i64 {
    with_vec(handle, -1, |v| {
        v.iter().rposition(|x| *x == needle).map(|i| i as i64).unwrap_or(-1)
    })
}

/// `arr.includes(needle)` → 1 ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES(handle: u64, needle: i64) -> i64 {
    with_vec(handle, 0, |v| if v.contains(&needle) { 1 } else { 0 })
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

/// `arr.splice(start, deleteCount)` — remove `deleteCount` elementos a
/// partir de `start` in-place. Retorna handle de Vec com os removidos.
/// Versao sem `...items` (insercao). Items vao em PR separada.
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
}
