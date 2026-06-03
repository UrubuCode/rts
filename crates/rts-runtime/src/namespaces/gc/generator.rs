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

use crate::namespaces::gc::handles::{Entry, GenStateData, alloc_entry, with_entry, with_entry_mut};

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
    // (#477) Se o handle eh um generator lazy (state-machine), delega.
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __RTS_FN_NS_GC_GEN_SM_NEXT(vec_handle);
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_RETURN(vec_handle: u64, value: i64) -> u64 {
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __RTS_FN_NS_GC_GEN_SM_RETURN(vec_handle, value);
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

/// `__RTS_GEN_GET_RET(vec)` — devolve o ret_value (`return X`) registrado
/// por uma generator fn finita, ou undefined se ausente. Usado por
/// `const r = yield* gen()` (#275/#379): o desugar empurra os elementos do
/// Vec delegado em __gen_buf e captura o ret_value em `r`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_GET_RET(vec_handle: u64) -> i64 {
    GEN_RETS.with(|c| c.borrow().get(&vec_handle).copied()).unwrap_or(UNDEFINED)
}

// ── Lazy state-machine (#477) ────────────────────────────────────────────────
// Generators infinitos / com control-flow exigem suspensao real. Aqui o desugar
// (generator_sm.rs) emite uma fn de estado `extern "C" fn(u64) -> i64` que, a
// cada chamada, avanca do estado atual ate o proximo `yield` e SUSPENDE,
// devolvendo o valor yieldado. Os locais vivem no `frame` do `Entry::GenState`.
// Esta camada coexiste com o eager-buffer (Entry::Vec): generators elegiveis
// (state-machine) usam GenState; os demais continuam no Vec.

/// Marca thread-local: alguns "value" yieldados sao i64 crus (numeros), nao
/// handles. Sentinela `done` eh comunicada pelo proprio GenState.done.

/// `__RTS_GEN_SM_NEW(fn_ptr, nslots)` — aloca um generator lazy com a fn de
/// estado `fn_ptr` e `nslots` slots de frame zerados. Os argumentos da
/// generator fn sao escritos depois via `GEN_SM_FSET`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_NEW(fn_ptr: u64, nslots: i64) -> u64 {
    let n = if nslots < 0 { 0 } else { nslots as usize };
    alloc_entry(Entry::GenState(Box::new(GenStateData {
        fn_ptr,
        state: 0,
        frame: vec![0i64; n],
        ret: UNDEFINED,
        done: false,
    })))
}

/// `__RTS_GEN_SM_FGET(h, idx)` — le o slot `idx` do frame.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_FGET(h: u64, idx: i64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.frame.get(idx as usize).copied().unwrap_or(0),
        _ => 0,
    })
}

/// `__RTS_GEN_SM_FSET(h, idx, val)` — escreve o slot `idx` do frame.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_FSET(h: u64, idx: i64, val: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            let i = idx as usize;
            if i < g.frame.len() {
                g.frame[i] = val;
            }
        }
    });
}

/// `__RTS_GEN_SM_STATE(h)` — le o label de retomada atual.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_STATE(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.state,
        _ => -1,
    })
}

/// `__RTS_GEN_SM_SETSTATE(h, s)` — grava o proximo label de retomada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_SETSTATE(h: u64, s: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.state = s;
        }
    });
}

/// `__RTS_GEN_SM_YIELD(h, val)` — marca o generator como suspenso (nao-done) e
/// devolve `val` (a fn de estado retorna esse valor para o NEXT). Pura
/// passthrough; existe para deixar o ponto de suspensao explicito no desugar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_YIELD(h: u64, val: i64) -> i64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = false;
        }
    });
    val
}

/// `__RTS_GEN_SM_DONE(h, ret)` — marca o generator como terminado, registra o
/// `return X` (ret) e devolve `ret`. A fn de estado retorna esse valor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_DONE(h: u64, ret: i64) -> i64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = true;
            g.ret = ret;
        }
    });
    ret
}

/// `gen.next()` para generators lazy (Entry::GenState). Invoca a fn de estado
/// (que avanca ate o proximo yield), le o flag `done` e monta `{value, done}`.
/// Se ja' terminado, devolve `{value:undefined, done:true}` sem reinvocar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_NEXT(h: u64) -> u64 {
    let (fn_ptr, already_done) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.fn_ptr, g.done),
        _ => (0u64, true),
    });
    if fn_ptr == 0 {
        return make_result(UNDEFINED, true);
    }
    if already_done {
        return make_result(UNDEFINED, true);
    }
    // A fn de estado pode chamar SM_DONE (set done=true) ou SM_YIELD (done=false)
    // antes de retornar. Default otimista: assume suspensao (yield).
    let state_fn: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let value = state_fn(h);
    let done = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.done,
        _ => true,
    });
    make_result(value, done)
}

/// `gen.return(v)` para generators lazy — encerra antecipadamente: marca done e
/// devolve `{value:v, done:true}`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_RETURN(h: u64, value: i64) -> u64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = true;
        }
    });
    make_result(value, true)
}

/// `__RTS_GEN_SM_DRAIN(h)` — consome o generator lazy ate `done`, coletando os
/// valores yieldados num `Entry::Vec`. Usado por for-of/spread sobre generator
/// state-machine (finito). O valor de `return X` NAO entra (semantica iterador).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_DRAIN(h: u64) -> u64 {
    let fn_ptr = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.fn_ptr,
        _ => 0,
    });
    let mut out: Vec<i64> = Vec::new();
    if fn_ptr != 0 {
        let state_fn: extern "C" fn(u64) -> i64 =
            unsafe { std::mem::transmute(fn_ptr as usize) };
        loop {
            let already_done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            if already_done {
                break;
            }
            let value = state_fn(h);
            let done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            if done {
                break;
            }
            out.push(value);
        }
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `__RTS_GEN_SM_IS(h)` — 1 se o handle eh um generator lazy (GenState), 0 senao.
/// Permite ao codegen rotear `.next()` p/ SM_NEXT vs o cursor lateral do Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_IS(h: u64) -> i64 {
    with_entry(h, |e| matches!(e, Some(Entry::GenState(_))) as i64)
}

/// Aloca o objeto-resultado `{value, done}` como Map.
fn make_result(value: i64, done: bool) -> u64 {
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("value".to_string(), value);
    m.insert("done".to_string(), if done { BOOL_TRUE } else { BOOL_FALSE });
    alloc_entry(Entry::Map(Box::new(m)))
}

// ── Iterator helpers (#306) ─────────────────────────────────────────────────
// `Iterator.from(arr)` cria um iterator-wrapper: clona o Vec e usa o cursor
// lateral (GEN_CURSORS) para rastrear consumo. `.toArray()` devolve os
// elementos restantes (cursor..len) e avança o cursor ao fim — a 2a chamada
// retorna vazio (iterator esgotado), igual a JS.

/// (#216/299) Metodo nativo `arr[Symbol.iterator]()` — recebe o array como
/// `this` (has_this_param=true no FunctionData) e devolve um iterator sobre
/// uma copia (mesmo backing de Iterator.from). Permite `for-of`/spread sobre
/// o resultado e protocolo iteravel manual (`it.next()`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_VALUES_ITER(this_arr: i64) -> i64 {
    __RTS_FN_GL_ITERATOR_FROM(this_arr as u64) as i64
}

/// (#216/299) Devolve um handle Function que, chamado com `this`=arr, produz
/// o iterator (ARRAY_VALUES_ITER). Usado por `arr[Symbol.iterator]` — o
/// resultado tem `typeof === "function"` e eh chamavel. So' faz sentido p/
/// Vec/array-like; caller decide quando emitir.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_ITERATOR_FN() -> u64 {
    use crate::namespaces::gc::handles::FunctionData;
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr: __RTS_FN_GL_ARRAY_VALUES_ITER as u64,
        arity: 1,
        name: "[Symbol.iterator]".into(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: true,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
    })))
}

/// `Iterator.from(vec)` — novo handle de iterator sobre uma cópia do Vec,
/// com cursor lateral em 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ITERATOR_FROM(vec_handle: u64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ITERATOR_TO_ARRAY(it_handle: u64) -> u64 {
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
