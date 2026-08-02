//! Vec<i64> — lista ordenada de valores i64.
//!
//! Os 9 membros do namespace `collections` referentes a Vec sao hand-written
//! (Fase 2, remoção da `rts-macro`): cada um eh um extern "C" + uma entrada em
//! `append_engine_members`. O resto sao non-member externs (`*_AUTO`,
//! `GL_ARRAY_*`, Array.prototype methods) que o codegen chama por simbolo. O
//! `register()` agregador do owner (`collections::mod`) chama
//! `map::append_engine_members` e depois `vec::append_engine_members`.

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};
use rts_engine::heap::slots::Slots;

fn with_vec<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&[i64]) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v.as_slice()),
        _ => default,
    })
}

fn with_vec_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Slots) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v),
        _ => default,
    })
}

// Membros Vec do namespace `collections` (parte; map.rs declara a outra).
// Hand-written (Fase 2): externs + `append_engine_members` (abaixo).

/// Creates an empty Vec<number>.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_NEW")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> Handle {
    alloc_entry(Entry::vec(Vec::new()))
}

/// Releases the vec handle.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_FREE")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_FREE(h: U64) {
    free_handle(h);
}

/// Number of elements; -1 if the handle is invalid.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_LEN")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_LEN(h: U64) -> I64 {
    with_vec(h, -1, |v| v.len() as i64)
}

/// Appends `value` to the end.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_PUSH")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(h: U64, value: I64) {
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
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_POP")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_POP(h: U64) -> I64 {
    let popped: Option<i64> = with_vec_mut(h, None, |v| v.pop());
    match popped {
        Some(v) => v,
        None => i64::MIN + 2,
    }
}

/// Element at `index`, or 0 out of range.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_GET")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_GET(h: U64, index: I64) -> I64 {
    if index < 0 {
        return 0;
    }
    with_vec(h, 0, |v| v.get(index as usize).copied().unwrap_or(0))
}

/// Writes `value` at `index`. No-op out of range.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_SET")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_SET(h: U64, index: I64, value: I64) {
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
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_CLEAR")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_CLEAR(h: U64) {
    with_vec_mut(h, (), |v| v.clear());
}

/// Junta os elementos do vec separados por `sep` (string handle). Cada elemento i64 e' tratado como string handle se valido, senao formatado como numero decimal. Retorna handle da string resultante.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_JOIN")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_JOIN(h: U64, sep: Handle) -> Handle {
    let elems: Option<Vec<i64>> = with_entry(h, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.iter().copied().collect()),
        _ => None,
    });
    let Some(elems) = elems else {
        return 0;
    };
    // sep handle 0 = argumento omitido → default JS "," (deixa o Registry
    // expressar `join(sep? = ",")` via DefaultArg::Int(0) sem hardcodar a
    // vírgula no codegen). Nenhum caller passa 0 para um sep real.
    let sep_bytes: Vec<u8> = if sep == 0 {
        b",".to_vec()
    } else {
        with_entry(sep, |entry| match entry {
            Some(Entry::String(b)) => b.clone(),
            _ => Vec::new(),
        })
    };
    let mut out: Vec<u8> = Vec::new();
    join_into(&mut out, &elems, &sep_bytes, 0);
    alloc_entry(Entry::String(out))
}

/// Constrói um `Member` (formato builder do `rts-engine`) para uma função do
/// namespace `collections`. Helper local — substitui o que a macro
/// `#[rts_namespace(part)]` gerava implicitamente.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Empurra os membros Vec desta `part` (formato builder do `rts-engine`) na
/// lista do owner. Chamado pelo `register()` agregador de `collections::mod`,
/// DEPOIS de `map::append_engine_members` — preserva a ordem map→vec do antigo
/// `concat_members`. Hand-written (Fase 2, remoção da `rts-macro`).
pub fn append_engine_members(v: &mut Vec<Member>) {
    v.push(func(
        "vec_new",
        "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
        Sig::new(Vec::new(), AbiType::Handle),
        "vec_new(): number",
        "Creates an empty Vec<number>.",
        __RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
    ));
    v.push(func(
        "vec_free",
        "__RTS_FN_NS_COLLECTIONS_VEC_FREE",
        Sig::new(vec![AbiType::U64], AbiType::Void),
        "vec_free(h: number): void",
        "Releases the vec handle.",
        __RTS_FN_NS_COLLECTIONS_VEC_FREE as *const u8,
    ));
    v.push(func(
        "vec_len",
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
        Sig::new(vec![AbiType::U64], AbiType::I64),
        "vec_len(h: number): number",
        "Number of elements; -1 if the handle is invalid.",
        __RTS_FN_NS_COLLECTIONS_VEC_LEN as *const u8,
    ));
    v.push(func(
        "vec_push",
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Void),
        "vec_push(h: number, value: number): void",
        "Appends `value` to the end.",
        __RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
    ));
    v.push(func(
        "vec_pop",
        "__RTS_FN_NS_COLLECTIONS_VEC_POP",
        Sig::new(vec![AbiType::U64], AbiType::I64),
        "vec_pop(h: number): number",
        "Removes and returns the last element; 0 when empty.",
        __RTS_FN_NS_COLLECTIONS_VEC_POP as *const u8,
    ));
    v.push(func(
        "vec_get",
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
        "vec_get(h: number, index: number): number",
        "Element at `index`, or 0 out of range.",
        __RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
    ));
    v.push(func(
        "vec_set",
        "__RTS_FN_NS_COLLECTIONS_VEC_SET",
        Sig::new(
            vec![AbiType::U64, AbiType::I64, AbiType::I64],
            AbiType::Void,
        ),
        "vec_set(h: number, index: number, value: number): void",
        "Writes `value` at `index`. No-op out of range.",
        __RTS_FN_NS_COLLECTIONS_VEC_SET as *const u8,
    ));
    v.push(func(
        "vec_clear",
        "__RTS_FN_NS_COLLECTIONS_VEC_CLEAR",
        Sig::new(vec![AbiType::U64], AbiType::Void),
        "vec_clear(h: number): void",
        "Removes all elements.",
        __RTS_FN_NS_COLLECTIONS_VEC_CLEAR as *const u8,
    ));
    v.push(func(
        "vec_join",
        "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
        Sig::new(vec![AbiType::U64, AbiType::Handle], AbiType::Handle),
        "vec_join(h: number, sep: number): string",
        "Junta os elementos do vec separados por `sep` (string handle). Cada elemento i64 e' tratado como string handle se valido, senao formatado como numero decimal. Retorna handle da string resultante.",
        __RTS_FN_NS_COLLECTIONS_VEC_JOIN as *const u8,
    ));
}

/// Limite duro de elementos por vec — protege contra OOM em cenarios
/// patologicos (ex: generator infinito desugared para buffer eager,
/// loop sem condicao de parada). 1M i64 = 8MiB por vec, suficiente
/// pro caso real e barato comparado aos GBs que um leak descontrolado
/// produz.
const VEC_MAX_LEN: usize = 1_000_000;

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
                out.extend_from_slice(rts_engine::numfmt::format_js_number(f).as_bytes());
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

/// `arr.splice(start, deleteCount?, ...items)` variádico — args empacotados num
/// Vec `[start, count?, ...items]` (o emitter genérico empacota tudo). count
/// omitido = remove até o fim (i64::MAX). Mutaciona o receiver in-place, devolve
/// os removidos. Move o desempacotamento start/count/items pro runtime.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_AUTO")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_SPLICE_AUTO(recv: u64, args_vec: u64) -> u64 {
    let args: Vec<i64> = with_vec(args_vec, Vec::new(), |v| v.to_vec());
    let start = args.first().copied().unwrap_or(0);
    let delete_count = if args.len() >= 2 { args[1] } else { i64::MAX };
    let items: Vec<i64> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        Vec::new()
    };
    let removed: Vec<i64> = with_vec_mut(recv, Vec::new(), |v| {
        let len = v.len() as i64;
        let s = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let count = delete_count.max(0).min(len - s as i64) as usize;
        let drained: Vec<i64> = v.drain_range(s..s + count);
        for (i, item) in items.into_iter().enumerate() {
            v.insert(s + i, item);
        }
        drained
    });
    alloc_entry(Entry::vec(removed))
}

/// `arr.toSpliced(start, deleteCount?, ...items)` variádico (imutável) — como
/// SPLICE_AUTO mas NÃO mutaciona o receiver; devolve um array novo com o splice
/// aplicado.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_AUTO")]
pub fn __RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_AUTO(recv: u64, args_vec: u64) -> u64 {
    let args: Vec<i64> = with_vec(args_vec, Vec::new(), |v| v.to_vec());
    let start = args.first().copied().unwrap_or(0);
    let delete_count = if args.len() >= 2 { args[1] } else { i64::MAX };
    let items: Vec<i64> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        Vec::new()
    };
    let mut out: Vec<i64> = with_vec(recv, Vec::new(), |v| v.to_vec());
    let len = out.len() as i64;
    let s = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    } as usize;
    let count = delete_count.max(0).min(len - s as i64) as usize;
    out.splice(s..s + count, items);
    alloc_entry(Entry::vec(out))
}

// (LAYERING FIX 2026-07-24) __RTS_FN_GL_ARRAY_NEW_WITH_LENGTH/FROM_LENGTH
// MUDARAM para `rts-primitives::array` (Array é PRIMORDIAL, e essas 2 não
// tocam nenhum estado non-primordial).
pub use rts_primitives::array::{
    __RTS_FN_GL_ARRAY_FROM_LENGTH, __RTS_FN_GL_ARRAY_NEW_WITH_LENGTH,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn handle_to_vec(h: u64) -> Vec<i64> {
        with_vec(h, Vec::new(), |v| v.to_vec())
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
}
