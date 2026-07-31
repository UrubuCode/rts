//! The EAGER generator protocol — `.next()` / `.value` / `.done` over the Vec a
//! finite generator function returns.
//!
//! On-demand cursor design: `generator_desugar` (parser) eager-buffers, `g()`
//! returns the **Vec** directly, and for-of/spread iterate it unchanged. `.next()`
//! reads and advances a side cursor keyed by handle ([`super::GEN_CURSORS`]).
//! Nothing here changes `g()`'s return type — the earlier attempt, which wrapped
//! the Vec, broke for-of over a simple body.
//!
//! INFINITE generators (`while (true) yield`) overflow the eager buffer and need
//! the real state machine — that is [`super::sm`] (#477).

use std::collections::HashMap;

use crate::heap::handles::{Entry, with_entry};

use super::sm::{__rtsn_gen_sm_next, __rtsn_gen_sm_return, __rtsn_gen_sm_throw};
use super::{BOOL_FALSE, BOOL_TRUE, GEN_CURSORS, GEN_RETS, UNDEFINED, make_result, read_result_parts};

unsafe extern "C" {
    /// `Function.prototype.call`-style invocation of a Function-class handle.
    ///
    /// A USER iterator object (`{ next() {...} }`) carries its own `next`, and
    /// the eager protocol has to invoke it. `Function` is a PRIMORDIAL class
    /// whose body correctly lives in `rts-primitives`, one layer ABOVE this
    /// crate, so this resolves by LINK — the JIT registers the symbol and the
    /// AOT staticlib exports it. Calling back into user code is not something
    /// the iteration protocol can own; owning the protocol is.
    fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64;
}


/// (motor NOVO) Lê o campo `value` do `{value, done}` (Entry::Map) que
/// `GENERATOR_NEXT`/`GEN_SM_NEXT` devolvem. O motor novo não lê um Entry::Map via
/// seu obj_get shape-based, então expõe acessores: a engine constrói o objeto
/// `{value, done}` do modelo NOVO a partir destes. `value` é o word
/// (yield → word PolyValue do motor novo; done-sem-valor → `UNDEFINED` sentinela,
/// que a engine remapeia pro undefined do motor novo).
#[rtse::abi(native, value = "iter_value")]
pub fn iter_value(result_handle: u64) -> i64 {
    read_result_parts(result_handle)
        .map(|(v, _)| v)
        .unwrap_or(UNDEFINED)
}

/// (motor NOVO) Lê o campo `done` como flag `1`/`0` (o Map guarda o sentinela
/// `BOOL_TRUE`/`BOOL_FALSE` era-i64; a engine faz o PolyValue bool do motor novo).
#[rtse::abi(native, value = "iter_done")]
pub fn iter_done(result_handle: u64) -> i64 {
    read_result_parts(result_handle)
        .map(|(_, d)| i64::from(d))
        .unwrap_or(1)
}


/// Whether `handle` is a Vec that was OPENED as an iterator — i.e. registered in
/// `GEN_CURSORS` at CREATION (`Iterator.from`, `arr.values()`), not merely one
/// that `generator_next` cursored on the way past.
///
/// The distinction matters and is easy to get wrong: `generator_next` uses
/// `or_insert(0)`, so it CREATES a cursor for whatever handle it is handed. Asking
/// "is there a cursor?" after the fact therefore answers yes for a plain array
/// too. This predicate is `contains_key` ONLY — it never inserts — so it stays a
/// property of how the handle was created. That is what lets the dynamic dispatch
/// accept `Entry::Vec` as an iterator without `[1,2].next()` starting to work
/// (issue #2042: a plain array must keep reading `.next` as `undefined`).
pub fn vec_is_open_iterator(handle: u64) -> bool {
    GEN_CURSORS.with(|c| c.borrow().contains_key(&handle))
}

/// Register `handle` as an OPEN ITERATOR positioned at element 0 — what
/// `arr.values()`/`keys()`/`entries()` and `Iterator.from` hand back. Pairs with
/// [`vec_is_open_iterator`]: this is the ONLY way a Vec becomes an iterator, so a
/// plain array never answers `.next()`.
pub fn open_vec_iterator(handle: u64) {
    GEN_CURSORS.with(|c| {
        c.borrow_mut().insert(handle, 0);
    });
}

/// `__RTS_GEN_FINISH(buf, ret)` — registra o ret_value do generator (do
/// `return X`) e devolve o proprio Vec. Chamado no `return` desugarado.
#[rtse::abi(native, value = "generator_set_ret")]
pub fn generator_set_ret(vec_handle: u64, ret: i64) -> u64 {
    // So' registra ret nao-undefined (evita poluir o side-table a toa).
    if ret != UNDEFINED {
        GEN_RETS.with(|c| {
            c.borrow_mut().insert(vec_handle, ret);
        });
    }
    vec_handle
}

/// `gen.next()` onde `gen` eh o Vec retornado por uma generator fn finita.
/// Avanca o cursor lateral do handle e devolve `{value, done}` (Map). Quando
/// o handle NAO eh um Vec (objeto com `.next()` proprio, etc), retorna
/// `{value:undefined, done:true}` — caller deve rotear so' p/ generator_vars.
#[rtse::abi(native, value = "generator_next")]
pub fn generator_next(vec_handle: u64) -> u64 {
    // (#477) Se o handle eh um generator lazy (state-machine), delega.
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __rtsn_gen_sm_next(vec_handle);
    }
    // (cross-runtime #344) The `.next()` routing was broadened to ambiguous
    // handle locals, so it also reaches plain-object custom iterators
    // (`const it = { next() {...} }`, a Map handle with an own `next` member).
    // Class-instance iterators are excluded upstream (local_class_ty), and their
    // method lives on the prototype, not the instance Map — so an own `next`
    // member here means a user iterator object. Invoke its method with
    // `this` = the object and return its `{value, done}` directly.
    let custom_next = with_entry(vec_handle, |e| match e {
        Some(Entry::Map(m)) => m.get("next").copied().filter(|&v| v != 0),
        _ => None,
    });
    if let Some(next_fn) = custom_next {
        return unsafe { __RTS_FN_GL_FUNCTION_CALL(next_fn as u64, vec_handle as i64, 0) } as u64;
    }
    let len = with_entry(vec_handle, |e| match e {
        Some(Entry::Vec(v)) => Some(v.len()),
        _ => None,
    });
    let Some(len) = len else {
        return make_result(UNDEFINED, true);
    };
    let cursor = GEN_CURSORS.with(|c| {
        let mut m = c.borrow_mut();
        let entry = m.entry(vec_handle).or_insert(0);
        let cur = *entry;
        if cur <= len {
            *entry = cur + 1;
        }
        cur
    });
    if cursor < len {
        let val = with_entry(vec_handle, |e| match e {
            Some(Entry::Vec(v)) => v[cursor],
            _ => UNDEFINED,
        });
        make_result(val, false)
    } else if cursor == len {
        // Primeiro next apos esgotar os yields: devolve o ret_value (`return
        // X`) com done:true. Se nao houver, undefined.
        let ret = GEN_RETS.with(|c| c.borrow().get(&vec_handle).copied()).unwrap_or(UNDEFINED);
        make_result(ret, true)
    } else {
        // Esgotado e ja' devolveu o ret: value undefined, done:true (JS spec).
        make_result(UNDEFINED, true)
    }
}

/// `gen.return(v)` — encerra o generator antecipadamente. Marca o cursor
/// como esgotado (proximos `.next()` dao done:true) e devolve `{value:v,
/// done:true}` — semantica JS. `for-of` que ja' terminou nao eh afetado.
#[rtse::abi(native, value = "generator_return")]
pub fn generator_return(vec_handle: u64, value: i64) -> u64 {
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __rtsn_gen_sm_return(vec_handle, value);
    }
    let len = with_entry(vec_handle, |e| match e {
        Some(Entry::Vec(v)) => Some(v.len()),
        _ => None,
    });
    // Marca esgotado: cursor > len pra que NEXT devolva undefined/done.
    if let Some(len) = len {
        GEN_CURSORS.with(|c| {
            c.borrow_mut().insert(vec_handle, len + 1);
        });
    }
    make_result(value, true)
}

/// `gen.throw(e)` — dispatcher: para generators lazy (GenState) delega a
/// `GEN_SM_THROW` (roda finally / absorve). Para o eager-buffer (Vec) nao ha'
/// try-region modelada; marca esgotado e propaga via error slot.
#[rtse::abi(native, value = "generator_throw")]
pub fn generator_throw(vec_handle: u64, err: i64) -> u64 {
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __rtsn_gen_sm_throw(vec_handle, err);
    }
    let len = with_entry(vec_handle, |e| match e {
        Some(Entry::Vec(v)) => Some(v.len()),
        _ => None,
    });
    if let Some(len) = len {
        GEN_CURSORS.with(|c| {
            c.borrow_mut().insert(vec_handle, len + 1);
        });
    }
    crate::collector::error::__rtsn_error_set(err as u64);
    make_result(err, true)
}

/// `__RTS_GEN_GET_RET(vec)` — devolve o ret_value (`return X`) registrado
/// por uma generator fn finita, ou undefined se ausente. Usado por
/// `const r = yield* gen()` (#275/#379): o desugar empurra os elementos do
/// Vec delegado em __gen_buf e captura o ret_value em `r`.
#[rtse::abi(native, value = "generator_get_ret")]
pub fn generator_get_ret(vec_handle: u64) -> i64 {
    GEN_RETS.with(|c| c.borrow().get(&vec_handle).copied()).unwrap_or(UNDEFINED)
}

