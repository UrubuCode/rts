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

/// `gen.throw(e)` — dispatcher: para generators lazy (GenState) delega a
/// `GEN_SM_THROW` (roda finally / absorve). Para o eager-buffer (Vec) nao ha'
/// try-region modelada; marca esgotado e propaga via error slot.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_THROW(vec_handle: u64, err: i64) -> u64 {
    let is_sm = with_entry(vec_handle, |e| matches!(e, Some(Entry::GenState(_))));
    if is_sm {
        return __RTS_FN_NS_GC_GEN_SM_THROW(vec_handle, err);
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
    crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(err as u64);
    make_result(err, true)
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
        finally_state: -1,
        pending_kind: 0,
        pending_val: UNDEFINED,
        is_async: false,
        result_promise: None,
        pending_await: None,
        awaited_val: UNDEFINED,
        awaited_rejected: false,
        sent: UNDEFINED,
        catch_state: -1,
        is_async_gen: false,
        next_promise: None,
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

/// `__RTS_GEN_SM_SENT(h)` — valor passado no ultimo `gen.next(v)` (#211
/// value-passing). Lido pela state-machine na retomada: `const x = yield E`
/// vira `yield E; x = SENT(g)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_SENT(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.sent,
        _ => UNDEFINED,
    })
}

/// `gen.next(v)` com argumento (#211 value-passing): injeta `v` como o valor
/// `sent` do generator e avanca. Para a primeira chamada (antes do 1o yield) o
/// valor eh ignorado pela maquina (nao ha' estado pos-yield que o leia), igual
/// a' spec JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GENERATOR_NEXT_SENT(h: u64, arg: i64) -> u64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.sent = arg;
        }
    });
    __RTS_FN_NS_GC_GENERATOR_NEXT(h)
}

/// `gen.next()` para generators lazy (Entry::GenState). Invoca a fn de estado
/// (que avanca ate o proximo yield), le o flag `done` e monta `{value, done}`.
/// Se ja' terminado, devolve `{value:undefined, done:true}` sem reinvocar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_NEXT(h: u64) -> u64 {
    // (cross-runtime #392) async generator: `.next()` devolve Promise<{value,
    // done}>; `await it.next()` desempacota e o for-await aguarda. Sync gen segue.
    let is_agen = with_entry(h, |e| matches!(e, Some(Entry::GenState(g)) if g.is_async_gen));
    if is_agen {
        return __RTS_FN_NS_GC_AGEN_NEXT(h);
    }
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

/// `gen.return(v)` para generators lazy (#477 fatia 2). Se ha' uma try-region
/// ativa com `finally`, redireciona a execucao para o finally (registra a
/// completion `return` pendente e re-invoca a fn de estado a partir do finally).
/// O `yield` dentro do finally INTERCEPTA o return -> devolve `{value:yield,
/// done:false}`. Sem finally ativo, encerra como antes: `{value:v, done:true}`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_RETURN(h: u64, value: i64) -> u64 {
    gen_sm_abrupt(h, value, 1)
}

/// `gen.throw(e)` para generators lazy (#477 fatia 2). Se ha' uma try-region
/// ativa com `finally`, redireciona para o finally (completion `throw`
/// pendente). O `yield` no finally ABSORVE o throw -> suspende devolvendo
/// `{value:yield, done:false}` SEM propagar a excecao. Sem finally ativo,
/// propaga: marca done e seta o error slot (caller re-lanca).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_THROW(h: u64, err: i64) -> u64 {
    // (#211 try/catch) Se ha' um catch ativo, salta para ele com `err` em
    // pending_val (lido via CAUGHT) e re-invoca. O catch absorve a excecao.
    let (fn_ptr, catch_state, done0) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.fn_ptr, g.catch_state, g.done),
        _ => (0u64, -1, true),
    });
    if catch_state >= 0 && !done0 && fn_ptr != 0 {
        with_entry_mut(h, |e| {
            if let Some(Entry::GenState(g)) = e {
                g.pending_val = err;
                g.state = catch_state;
                g.catch_state = -1; // throw dentro do catch propaga (nao re-loop)
            }
        });
        let state_fn: extern "C" fn(u64) -> i64 =
            unsafe { std::mem::transmute(fn_ptr as usize) };
        let yielded = state_fn(h);
        let done = with_entry(h, |e| match e {
            Some(Entry::GenState(g)) => g.done,
            _ => true,
        });
        return make_result(yielded, done);
    }
    let has_finally = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.finally_state >= 0 && !g.done,
        _ => false,
    });
    if has_finally {
        return gen_sm_abrupt(h, err, 2);
    }
    // Sem finally: propaga a excecao via error slot global (caller re-lanca).
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = true;
        }
    });
    crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(err as u64);
    make_result(err, true)
}

/// Logica comum de `.return`/`.throw`: redireciona para o finally se ativo,
/// senao encerra. `kind`: 1=return, 2=throw.
fn gen_sm_abrupt(h: u64, value: i64, kind: i64) -> u64 {
    let (fn_ptr, finally_state, already_done) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.fn_ptr, g.finally_state, g.done),
        _ => (0u64, -1, true),
    });
    if already_done || finally_state < 0 || fn_ptr == 0 {
        // Sem try-region ativa: encerra (semantica simples).
        with_entry_mut(h, |e| {
            if let Some(Entry::GenState(g)) = e {
                g.done = true;
            }
        });
        return make_result(value, true);
    }
    // Redireciona para o finally: registra a completion pendente e re-invoca.
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.pending_kind = kind;
            g.pending_val = value;
            g.state = finally_state;
            g.finally_state = -1; // nao re-disparar a mesma try-region
        }
    });
    let state_fn: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let yielded = state_fn(h);
    let done = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.done,
        _ => true,
    });
    make_result(yielded, done)
}

/// `__RTS_GEN_SM_ENTER_TRY(h, finally_state)` — registra o estado de entrada do
/// finally da try-region que estamos comecando. `.return`/`.throw` redirecionam
/// para esse estado se a suspensao ocorrer dentro da try.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_ENTER_TRY(h: u64, finally_state: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.finally_state = finally_state;
        }
    });
}

/// `__RTS_GEN_SM_ENTER_TRY_CATCH(h, catch_state)` (#211) — registra o estado de
/// entrada do `catch`. `.throw(e)` suspenso dentro da try salta para ele.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH(h: u64, catch_state: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.catch_state = catch_state;
        }
    });
}

/// `__RTS_GEN_SM_EXIT_TRY_CATCH(h)` (#211) — limpa o catch ativo (saida normal
/// do try body, sem excecao). Um `.throw` posterior nao roteia mais p/ esse
/// catch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH(h: u64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.catch_state = -1;
        }
    });
}

/// `__RTS_GEN_SM_CAUGHT(h)` (#211) — valor da excecao capturada (a binding do
/// `catch (e)`). Lido no inicio do estado do catch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_CAUGHT(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.pending_val,
        _ => UNDEFINED,
    })
}

/// `__RTS_GEN_SM_END_FINALLY(h)` — chamado ao fim do bloco finally. Limpa a
/// try-region ativa e honra a completion pendente: para `return`, retorna
/// `DONE(pending_val)`; para `throw`, seta o error slot e marca done. Devolve o
/// valor que a fn de estado deve retornar (e que NEXT empacotara em {value,done}).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_END_FINALLY(h: u64) -> i64 {
    let (kind, val) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.pending_kind, g.pending_val),
        _ => (0, UNDEFINED),
    });
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.finally_state = -1;
            g.pending_kind = 0;
        }
    });
    match kind {
        1 => {
            // return pendente: completion. state=-2 sinaliza ao desugar que a fn
            // de estado deve retornar `val` imediatamente (done) sem seguir p/ o
            // estado normal apos a try/finally.
            with_entry_mut(h, |e| {
                if let Some(Entry::GenState(g)) = e {
                    g.done = true;
                    g.ret = val;
                    g.state = -2;
                }
            });
            val
        }
        2 => {
            // throw pendente nao absorvido: propaga via error slot.
            with_entry_mut(h, |e| {
                if let Some(Entry::GenState(g)) = e {
                    g.done = true;
                    g.state = -2;
                }
            });
            crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(val as u64);
            val
        }
        _ => UNDEFINED, // sem completion pendente: caller segue p/ estado normal.
    }
}

/// `__RTS_GEN_SM_DRAIN(h)` — consome o generator lazy ate `done`, coletando os
/// valores yieldados num `Entry::Vec`. Usado por for-of/spread sobre generator
/// state-machine (finito). O valor de `return X` NAO entra (semantica iterador).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_SM_DRAIN(h: u64) -> u64 {
    // (cross-runtime #392) async generator: drena via AGEN_NEXT (que bombeia
    // alem dos awaits internos e devolve Promise<{value,done}> ja' resolvida).
    // Coleta os valores yieldados num Vec; se o corpo lancar, AGEN_NEXT rejeita
    // -> PROMISE_WAIT seta o error slot -> paramos (o for-of itera o que ja' foi
    // coletado e o try/catch ao redor pega a excecao no fall-through). Sem isto
    // a drena chamaria a state-fn crua e empurraria os dummies dos awaits (0).
    let is_agen = with_entry(h, |e| matches!(e, Some(Entry::GenState(g)) if g.is_async_gen));
    if is_agen {
        let mut out: Vec<i64> = Vec::new();
        loop {
            let done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            if done {
                break;
            }
            let p = __RTS_FN_NS_GC_AGEN_NEXT(h);
            let result_map = crate::namespaces::promise::ops::__RTS_FN_NS_PROMISE_WAIT(p);
            // Throw dentro do corpo: AGEN_NEXT rejeitou -> error slot setado.
            if crate::namespaces::gc::error::__RTS_FN_RT_ERROR_GET() != 0 {
                break;
            }
            let (val, dn) = with_entry(result_map as u64, |e| match e {
                Some(Entry::Map(m)) => (
                    m.get("value").copied().unwrap_or(UNDEFINED),
                    m.get("done").copied() == Some(BOOL_TRUE),
                ),
                _ => (UNDEFINED, true),
            });
            if dn {
                break;
            }
            out.push(val);
        }
        return alloc_entry(Entry::Vec(Box::new(out)));
    }
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

// ── Async state-machine (#207 fatia 1) ──────────────────────────────────────
// Uma `async function` elegivel vira a MESMA state-machine de generators, mas
// `await x` eh um ponto de suspensao que CEDE a microtask queue (em vez de
// `yield` que devolve ao .next() do caller). O START roda o corpo ate o 1o
// await, suspende, enfileira um AsyncResume sobre a promise awaited e devolve a
// promise-resultado. Quando a awaited settla, o drain injeta o valor e re-step.
// Isso produz o interleaving cooperativo (393): duas async fns concorrentes
// alternam a cada await.

/// `__RTS_GEN_SM_ASYNC_START(fn_ptr, nslots)` — aloca o GenState async (igual a
/// GEN_SM_NEW mas com `is_async=true`). Os params sao escritos via FSET pelo
/// ctor sintetico; depois o ctor chama ASYNC_STEP_INIT para rodar o 1o trecho.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ASYNC_SM_NEW(fn_ptr: u64, nslots: i64) -> u64 {
    let n = if nslots < 0 { 0 } else { nslots as usize };
    alloc_entry(Entry::GenState(Box::new(GenStateData {
        fn_ptr,
        state: 0,
        frame: vec![0i64; n],
        ret: UNDEFINED,
        done: false,
        finally_state: -1,
        pending_kind: 0,
        pending_val: UNDEFINED,
        is_async: true,
        result_promise: None,
        pending_await: None,
        awaited_val: UNDEFINED,
        awaited_rejected: false,
        sent: UNDEFINED,
        catch_state: -1,
        is_async_gen: false,
        next_promise: None,
    })))
}

/// (cross-runtime #392) `__RTS_AGEN_NEW(fn_ptr, nslots)` — aloca um async
/// generator lazy: yield (gera valores) + await (suspende). Lazy: nao roda nada
/// ate o 1o `.next()`. `is_async=true` habilita SUSPEND/AWAITED; `is_async_gen`
/// roteia o driver `agen_step`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_AGEN_NEW(fn_ptr: u64, nslots: i64) -> u64 {
    let n = if nslots < 0 { 0 } else { nslots as usize };
    alloc_entry(Entry::GenState(Box::new(GenStateData {
        fn_ptr,
        state: 0,
        frame: vec![0i64; n],
        ret: UNDEFINED,
        done: false,
        finally_state: -1,
        pending_kind: 0,
        pending_val: UNDEFINED,
        is_async: true,
        result_promise: None,
        pending_await: None,
        awaited_val: UNDEFINED,
        awaited_rejected: false,
        sent: UNDEFINED,
        catch_state: -1,
        is_async_gen: true,
        next_promise: None,
    })))
}

/// (cross-runtime #392) `gen.next()` de um async generator: devolve uma
/// `Promise<{value,done}>` JA' RESOLVIDA. Dispara o driver e, se o corpo
/// suspende num `await` interno, BOMBEIA o event loop (microtasks/timers) ate a
/// next_promise settle — mesmo padrao do `promise.wait`. Isso evita o deadlock
/// onde o caller (`await it.next()`) e o await interno do gen dependem do mesmo
/// drain: aqui o drain roda inline antes de retornar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_AGEN_NEXT(h: u64) -> u64 {
    use crate::namespaces::gc::promise_slot;
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
    let already_done = with_entry_mut(h, |e| match e {
        Some(Entry::GenState(g)) => {
            g.next_promise = Some(result.clone());
            g.done
        }
        _ => true,
    });
    if already_done {
        let r = make_result(UNDEFINED, true);
        promise_slot::resolve(&result, r as i64);
        return result_handle;
    }
    agen_step(h);
    // Bombeia ate a next_promise settle (gen suspendeu em await interno).
    if promise_slot::current_state(&result) == promise_slot::STATE_PENDING {
        use std::time::{Duration, Instant};
        use crate::namespaces::globals::timers::instance as timers;
        let cap = Instant::now() + Duration::from_secs(5);
        while promise_slot::current_state(&result) == promise_slot::STATE_PENDING {
            crate::namespaces::globals::text_encoding::instance::drain_microtasks();
            timers::pump_due_macrotasks();
            if promise_slot::current_state(&result) != promise_slot::STATE_PENDING {
                break;
            }
            let now = Instant::now();
            if now >= cap {
                break;
            }
            if let Some(next) = timers::next_macrotask_deadline() {
                let wake = next.min(cap);
                if wake > now {
                    std::thread::sleep((wake - now).min(Duration::from_millis(20)));
                }
            } else {
                // Sem timers pendentes: pequena cedencia pra tasks tokio
                // (Promise.resolve/all resolvidas por threads) settarem.
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    result_handle
}

/// Driver do async generator: invoca a state-fn (avanca ate o proximo yield,
/// await, ou done). Suspendeu em await => enfileira retomada (next_promise fica
/// pendente). Alcancou yield/done => resolve a next_promise com {value,done}
/// (ou rejeita se o corpo lancou).
fn agen_step(h: u64) {
    use crate::namespaces::gc::promise_slot;
    let fn_ptr = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.fn_ptr,
        _ => 0,
    });
    if fn_ptr == 0 {
        return;
    }
    let state_fn: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let rv = {
        let _aw = crate::runtime::async_rt::AsyncWorkerGuard::enter();
        state_fn(h)
    };
    let (done, pending, np) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => {
            (g.done, g.pending_await.clone(), g.next_promise.clone())
        }
        _ => (true, None, None),
    });
    if let Some(src) = pending {
        crate::namespaces::globals::text_encoding::instance::enqueue_microtask_async_resume(h, src);
        return;
    }
    if let Some(np) = np {
        let err = crate::namespaces::gc::error::__RTS_FN_RT_ERROR_GET();
        if err != 0 {
            crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
            promise_slot::reject(&np, err as i64);
        } else {
            let res = make_result(rv, done);
            promise_slot::resolve(&np, res as i64);
        }
        with_entry_mut(h, |e| {
            if let Some(Entry::GenState(g)) = e {
                g.next_promise = None;
            }
        });
    }
}

/// `__RTS_GEN_SM_ASYNC_START(h)` — aloca a promise-resultado pendente, guarda no
/// GenState, roda o 1o passo (ate o 1o await ou ate terminar) e devolve o HANDLE
/// da promise-resultado (preserva ABI `f(args) -> Promise`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ASYNC_SM_START(h: u64) -> u64 {
    use crate::namespaces::gc::promise_slot;
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.result_promise = Some(result.clone());
        }
    });
    async_sm_step(h);
    result_handle
}

/// Roda um passo da async SM: invoca a state-fn (avanca ate o proximo await ou
/// ate `return`). Apos retornar: se suspendeu (pending_await setado), enfileira
/// um AsyncResume sobre a awaited; se terminou (done), settla a result_promise.
fn async_sm_step(h: u64) {
    use crate::namespaces::gc::promise_slot;
    // (cross-runtime #392) async generator: driver proprio (resolve a Promise do
    // .next() corrente a cada yield, nao apenas no done).
    let is_agen = with_entry(h, |e| matches!(e, Some(Entry::GenState(g)) if g.is_async_gen));
    if is_agen {
        agen_step(h);
        return;
    }
    let fn_ptr = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.fn_ptr,
        _ => 0,
    });
    if fn_ptr == 0 {
        return;
    }
    let state_fn: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    // (cross-runtime #365) o corpo da async fn roda aqui; marca a thread como
    // async-worker pra que `parallel.map` use o caminho sequencial. Instalar
    // no pool rayon de dentro do corpo async crasha (workers nao registrados
    // na GC thread_registry chamando fn_ptr JIT).
    let ret = {
        let _aw = crate::runtime::async_rt::AsyncWorkerGuard::enter();
        state_fn(h)
    };
    // Le o estado pos-step.
    let (done, pending, result) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => {
            (g.done, g.pending_await.clone(), g.result_promise.clone())
        }
        _ => (true, None, None),
    });
    if done {
        // Corpo terminou (RESOLVE chamado). Settla a promise-resultado: se o
        // error slot esta setado (throw), rejeita; senao resolve com ret.
        if let Some(rp) = result {
            let err = crate::namespaces::gc::error::__RTS_FN_RT_ERROR_GET();
            if err != 0 {
                crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
                promise_slot::reject(&rp, err as i64);
            } else {
                promise_slot::resolve(&rp, ret);
            }
        }
        return;
    }
    // Suspendeu em await: enfileira AsyncResume sobre a promise awaited.
    if let Some(src) = pending {
        crate::namespaces::globals::text_encoding::instance::enqueue_microtask_async_resume(h, src);
    }
}

/// `__RTS_GEN_SM_ASYNC_SUSPEND(h, promise_handle)` — chamado pela state-fn ao
/// atingir `await x`. Extrai o Arc<PromiseSlot> do handle (ou cria um ja settled
/// se o valor awaited NAO eh uma Promise) e guarda em pending_await. Devolve um
/// valor dummy (a state-fn ja vai retornar logo apos).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ASYNC_SM_SUSPEND(h: u64, promise_handle: i64) -> i64 {
    use crate::namespaces::gc::promise_slot;
    // Extrai o slot da promise awaited. Se o handle nao eh PromiseAsync,
    // trata o valor como ja-resolvido (await de nao-Promise).
    let slot = with_entry(promise_handle as u64, |e| match e {
        Some(Entry::PromiseAsync(p)) => Some(p.clone()),
        _ => None,
    });
    let src = slot.unwrap_or_else(|| promise_slot::new_fulfilled(promise_handle));
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.pending_await = Some(src);
        }
    });
    0
}

/// `__RTS_GEN_SM_ASYNC_AWAITED(h)` — devolve o valor injetado pela retomada do
/// await. Se a promise awaited rejeitou, seta o error slot (await relanca).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ASYNC_SM_AWAITED(h: u64) -> i64 {
    let (val, rejected) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.awaited_val, g.awaited_rejected),
        _ => (UNDEFINED, false),
    });
    if rejected {
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(val as u64);
    }
    val
}

/// `__RTS_GEN_SM_ASYNC_RESOLVE(h, val)` — chamado pela state-fn no `return val`
/// (ou fim do corpo). Marca done e guarda ret; o async_sm_step settla a
/// result_promise apos a state-fn retornar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ASYNC_SM_RESOLVE(h: u64, val: i64) -> i64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = true;
            g.ret = val;
        }
    });
    val
}

/// Chamado pelo drain quando a promise awaited settla: injeta o valor/erro no
/// GenState e roda o proximo passo da async SM. Publico para `instance.rs`.
pub fn async_sm_resume(h: u64, value: i64, rejected: bool) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.awaited_val = value;
            g.awaited_rejected = rejected;
            g.pending_await = None;
        }
    });
    async_sm_step(h);
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
        rest_param_idx: -1,
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

// ── yield* lazy delegation (#477/#211) ───────────────────────────────────────
// `yield* SRC` na state-machine: em vez de materializar SRC inteiro (eager,
// estoura em delegado infinito), iteramos SRC um valor por vez e RE-YIELDAMOS
// cada um, suspendendo entre eles. O estado do iterador delegado vive
// runtime-side (keyed pelo handle): para um GenState eh o proprio `.done`; para
// um Vec eh o cursor em GEN_CURSORS. So' o HANDLE entra no frame do generator
// externo, entao a delegacao sobrevive a suspensao/retomada do externo.

thread_local! {
    /// Done-flag do ultimo DELEGATE_NEXT por handle de iterador delegado.
    static DELEGATE_DONE: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
}

fn set_delegate_done(h: u64, d: bool) {
    DELEGATE_DONE.with(|c| {
        c.borrow_mut().insert(h, d);
    });
}

/// `__RTS_GEN_DELEGATE_START(src)` — normaliza a fonte de um `yield*` num handle
/// de iterador. Generator (GenState) -> ele mesmo (ja' eh iterador). Array (Vec)
/// -> uma copia com cursor (ITERATOR_FROM). String -> Vec de chars (1 handle por
/// caractere). Outro -> passthrough (assume iterador).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_DELEGATE_START(src: i64) -> i64 {
    let h = src as u64;
    let kind = with_entry(h, |e| match e {
        Some(Entry::GenState(_)) => 1u8,
        Some(Entry::Vec(_)) => 2,
        Some(Entry::String(_)) => 3,
        _ => 0,
    });
    match kind {
        1 => src, // generator: ja' eh iterador
        2 => __RTS_FN_GL_ITERATOR_FROM(h) as i64,
        3 => {
            // String iteravel: cada char vira um handle de String de 1 char.
            let s: String = with_entry(h, |e| match e {
                Some(Entry::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                _ => String::new(),
            });
            let chars: Vec<i64> = s
                .chars()
                .map(|c| alloc_entry(Entry::String(c.to_string().into_bytes())) as i64)
                .collect();
            let vh = alloc_entry(Entry::Vec(Box::new(chars)));
            GEN_CURSORS.with(|c| {
                c.borrow_mut().insert(vh, 0);
            });
            vh as i64
        }
        _ => src,
    }
}

/// `__RTS_GEN_DELEGATE_NEXT(it)` — avanca o iterador delegado um passo. Devolve o
/// valor produzido (ou UNDEFINED se esgotou); o flag done fica acessivel via
/// `__RTS_GEN_DELEGATE_DONE(it)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_DELEGATE_NEXT(it: i64) -> i64 {
    let h = it as u64;
    let kind = with_entry(h, |e| match e {
        Some(Entry::GenState(_)) => 1u8,
        Some(Entry::Vec(_)) => 2,
        _ => 0,
    });
    match kind {
        1 => {
            let (fn_ptr, done0) = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => (g.fn_ptr, g.done),
                _ => (0u64, true),
            });
            if fn_ptr == 0 || done0 {
                set_delegate_done(h, true);
                return UNDEFINED;
            }
            let state_fn: extern "C" fn(u64) -> i64 =
                unsafe { std::mem::transmute(fn_ptr as usize) };
            let v = state_fn(h);
            let done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            set_delegate_done(h, done);
            if done { UNDEFINED } else { v }
        }
        2 => {
            let len = with_entry(h, |e| match e {
                Some(Entry::Vec(v)) => v.len(),
                _ => 0,
            });
            let cursor = GEN_CURSORS.with(|c| *c.borrow_mut().entry(h).or_insert(0));
            if cursor < len {
                let val = with_entry(h, |e| match e {
                    Some(Entry::Vec(v)) => v.get(cursor).copied().unwrap_or(UNDEFINED),
                    _ => UNDEFINED,
                });
                GEN_CURSORS.with(|c| {
                    if let Some(cur) = c.borrow_mut().get_mut(&h) {
                        *cur += 1;
                    }
                });
                set_delegate_done(h, false);
                val
            } else {
                set_delegate_done(h, true);
                UNDEFINED
            }
        }
        _ => {
            set_delegate_done(h, true);
            UNDEFINED
        }
    }
}

/// `__RTS_SYMBOL_ITERATOR_OF(obj)` (#222) — leitura de `obj[Symbol.iterator]`
/// ciente do tipo: devolve um handle de Function (para `typeof === "function"`)
/// se `obj` for iteravel (Vec/String/Map), senao UNDEFINED. Sem isto, todo
/// `obj[Symbol.iterator]` rendia uma function (mesmo para numeros), quebrando
/// `typeof item[Symbol.iterator] === "function"` (flatten recursava em numeros).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_SYMBOL_ITERATOR_OF(obj: i64) -> i64 {
    let h = obj as u64;
    let iterable = with_entry(h, |e| {
        matches!(
            e,
            Some(Entry::Vec(_)) | Some(Entry::String(_)) | Some(Entry::Map(_))
        )
    });
    if iterable {
        __RTS_FN_GL_ARRAY_ITERATOR_FN() as i64
    } else {
        UNDEFINED
    }
}

/// `__RTS_GEN_DELEGATE_DONE(it)` — 1 se o ultimo NEXT esgotou o delegado, senao
/// 0. Inteiro puro (nao sentinela de bool) para ser usado direto como condicao
/// truthy no `if` da state-machine.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GEN_DELEGATE_DONE(it: i64) -> i64 {
    let h = it as u64;
    let done = DELEGATE_DONE.with(|c| c.borrow().get(&h).copied().unwrap_or(false));
    if done { 1 } else { 0 }
}
