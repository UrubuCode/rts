//! Generator MVP (finito) — protocolo `.next()/.value/.done`.
//!
//! Abordagem ON-DEMAND: o `generator_desugar` (parser) continua fazendo
//! eager-buffer e `g()` retorna o **Vec** diretamente — for-of/spread iteram
//! o Vec como hoje, SEM mudanca. Para `.next()`, mantemos um cursor lateral
//! por-handle num HashMap thread-local. `GENERATOR_NEXT(vec_handle)` le/avanca
//! esse cursor e devolve `{value,done}` (Map). Assim nao alteramos o tipo de
//! retorno de g() nem o for-of (evita a regressao da tentativa anterior, onde
//! envolver o Vec num wrapper quebrava o for-of de corpo simples).
//!
//! Generators INFINITOS (`while(true) yield`) estouram o eager-buffer e exigem
//! state-machine real — follow-up (#477).

use std::cell::RefCell;
use std::collections::HashMap;

use indexmap::IndexMap;

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

/// Sentinel `undefined` (i64::MIN+2) — convencao do codegen/INSPECT.
const UNDEFINED: i64 = i64::MIN + 2;
/// Sentinels de bool: MIN = false, MIN+1 = true (igual TPL_COERCE_AUTO).
const BOOL_FALSE: i64 = i64::MIN;
const BOOL_TRUE: i64 = i64::MIN + 1;

thread_local! {
    /// Cursor de iteracao por handle de Vec consumido via `.next()`.
    static GEN_CURSORS: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
    /// Valor de `return X` do generator por handle de Vec. Devolvido pelo
    /// primeiro `.next()` apos esgotar os yields (`{value:X, done:true}`).
    static GEN_RETS: RefCell<HashMap<u64, i64>> = RefCell::new(HashMap::new());
}

/// `__RTS_GEN_FINISH(buf, ret)` — registra o ret_value do generator (do
/// `return X`) e devolve o proprio Vec. Chamado no `return` desugarado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_SET_RET(vec_handle: u64, ret: i64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_NEXT(vec_handle: u64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_RETURN(vec_handle: u64, value: i64) -> u64 {
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

/// Aloca o objeto-resultado `{value, done}` como Map.
fn make_result(value: i64, done: bool) -> u64 {
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("value".to_string(), value);
    m.insert("done".to_string(), if done { BOOL_TRUE } else { BOOL_FALSE });
    alloc_entry(Entry::Map(Box::new(m)))
}
