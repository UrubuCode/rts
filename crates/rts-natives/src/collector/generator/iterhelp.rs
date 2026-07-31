//! Iterator helpers (#306) — `arr.values()/keys()/entries()`, `Iterator.from`
//! and `.toArray()`.
//!
//! `Iterator.from(arr)` builds an iterator wrapper: it clones the Vec and tracks
//! consumption through the side cursor ([`super::GEN_CURSORS`]), which is what
//! keeps a plain array from answering `.next()`.

use crate::heap::handles::{Entry, alloc_entry, with_entry};

use super::{GEN_CURSORS, UNDEFINED, eager::open_vec_iterator};

// ── Iterator helpers (#306) ─────────────────────────────────────────────────
// `Iterator.from(arr)` cria um iterator-wrapper: clona o Vec e usa o cursor
// lateral (GEN_CURSORS) para rastrear consumo. `.toArray()` devolve os
// elementos restantes (cursor..len) e avança o cursor ao fim — a 2a chamada
// retorna vazio (iterator esgotado), igual a JS.

/// (#216/299) Metodo nativo `arr[Symbol.iterator]()` — recebe o array como
/// `this` (has_this_param=true no FunctionData) e devolve um iterator sobre
/// uma copia (mesmo backing de Iterator.from). Permite `for-of`/spread sobre
/// o resultado e protocolo iteravel manual (`it.next()`).
#[rtse::abi(global = "Array", value = "values_iter")]
pub fn values_iter(this_arr: i64) -> i64 {
    __rtsm_global_Iterator_from(this_arr as u64) as i64
}

/// (#216/299) Devolve um handle Function que, chamado com `this`=arr, produz
/// o iterator (ARRAY_VALUES_ITER). Usado por `arr[Symbol.iterator]` — o
/// resultado tem `typeof === "function"` e eh chamavel. So' faz sentido p/
/// Vec/array-like; caller decide quando emitir.
#[rtse::abi(global = "Array", value = "iterator_fn")]
pub fn iterator_fn() -> u64 {
    use rts_engine::heap::handles::FunctionData;
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr: __rtsm_global_Array_values_iter as *const () as u64,
        arity: 1,
        name: "[Symbol.iterator]".into(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: true,
        param_kinds: Vec::new(),
        // The iterator is a HANDLE, not a number: `return_kind: 0` (i64) let the
        // legacy invoker box it as a plain number, so `arr[Symbol.iterator]()`
        // read `typeof === "number"` and had no `.next` (issue #2042).
        return_kind: 5,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: false,
    })))
}

/// `Iterator.from(vec)` — novo handle de iterator sobre uma cópia do Vec,
/// com cursor lateral em 0.
#[rtse::abi(global = "Iterator", value = "from")]
pub fn from(vec_handle: u64) -> u64 {
    let items: Vec<i64> = with_entry(vec_handle, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let h = alloc_entry(Entry::Vec(Box::new(items)));
    GEN_CURSORS.with(|c| {
        c.borrow_mut().insert(h, 0);
    });
    h
}

/// `iterator.toArray()` — devolve um novo Vec com os elementos do cursor
/// ate o fim; avanca o cursor ao fim (esgota o iterator).
#[rtse::abi(global = "Iterator", value = "to_array")]
pub fn to_array(it_handle: u64) -> u64 {
    let all: Vec<i64> = with_entry(it_handle, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let cursor = GEN_CURSORS.with(|c| {
        let mut m = c.borrow_mut();
        let entry = m.entry(it_handle).or_insert(0);
        let cur = *entry;
        *entry = all.len(); // esgota
        cur
    });
    let rest: Vec<i64> = if cursor < all.len() {
        all[cursor..].to_vec()
    } else {
        Vec::new()
    };
    alloc_entry(Entry::Vec(Box::new(rest)))
}

