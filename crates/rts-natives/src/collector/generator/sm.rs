//! The LAZY state machine (#477) — a generator as a resumable frame instead of
//! an eager buffer, which is what makes an infinite generator possible.
//!
//! The state lives in `Entry::GenState`: a program counter, a slot array for the
//! locals that must survive a suspension, and the try/catch/finally bookkeeping
//! a `yield` inside a `try` needs. The codegen lowers the generator body to a
//! function that reads `gen_sm_state`, jumps, and writes it back before
//! returning — every `__rtsn_gen_sm_*` here is one edge of that protocol.
//!
//! Async generators reach the scheduler through [`super::AgenDriver`]; see the
//! module doc for why that indirection exists rather than a direct call.

use crate::heap::handles::{Entry, GenStateData, alloc_entry, with_entry, with_entry_mut};

use super::eager::__rtsn_generator_next;
use super::{UNDEFINED, agen_driver, is_async_gen, make_result, read_result_parts};

/// `undefined` como PolyValue WORD. O `sent` do generator cruza para a
/// state-machine por `SM_SENT`, que o codegen tipa como WORD — o sentinela
/// legado `UNDEFINED` (`i64::MIN+2`) NAO e' um word boxado: lido como
/// PolyValue ele e' um double inline (`-1e-323`), e era isso que um
/// `const a = yield x` via na primeira retomada em vez de `undefined`.
const SENT_UNDEFINED: i64 = crate::heap::poly::POLY_UNDEFINED as i64;

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
#[rtse::abi(native, value = "gen_sm_new")]
pub fn gen_sm_new(fn_ptr: u64, nslots: i64) -> u64 {
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
        sent: SENT_UNDEFINED,
        catch_state: -1,
        is_async_gen: false,
        next_promise: None,
    })))
}

/// `__RTS_GEN_SM_FGET(h, idx)` — le o slot `idx` do frame.
#[rtse::abi(native, value = "gen_sm_fget")]
pub fn gen_sm_fget(h: u64, idx: i64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.frame.get(idx as usize).copied().unwrap_or(0),
        _ => 0,
    })
}

/// `__RTS_GEN_SM_FSET(h, idx, val)` — escreve o slot `idx` do frame.
#[rtse::abi(native, value = "gen_sm_fset")]
pub fn gen_sm_fset(h: u64, idx: i64, val: i64) {
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
#[rtse::abi(native, value = "gen_sm_state")]
pub fn gen_sm_state(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.state,
        _ => -1,
    })
}

/// `__RTS_GEN_SM_SETSTATE(h, s)` — grava o proximo label de retomada.
#[rtse::abi(native, value = "gen_sm_setstate")]
pub fn gen_sm_setstate(h: u64, s: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.state = s;
        }
    });
}

/// `__RTS_GEN_SM_YIELD(h, val)` — marca o generator como suspenso (nao-done) e
/// devolve `val` (a fn de estado retorna esse valor para o NEXT). Pura
/// passthrough; existe para deixar o ponto de suspensao explicito no desugar.
#[rtse::abi(native, value = "gen_sm_yield")]
pub fn gen_sm_yield(h: u64, val: i64) -> i64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = false;
        }
    });
    val
}

/// `__RTS_GEN_SM_DONE(h, ret)` — marca o generator como terminado, registra o
/// `return X` (ret) e devolve `ret`. A fn de estado retorna esse valor.
#[rtse::abi(native, value = "gen_sm_done")]
pub fn gen_sm_done(h: u64, ret: i64) -> i64 {
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
#[rtse::abi(native, value = "gen_sm_sent")]
pub fn gen_sm_sent(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.sent,
        _ => UNDEFINED,
    })
}

/// `gen.next(v)` com argumento (#211 value-passing): injeta `v` como o valor
/// `sent` do generator e avanca. Para a primeira chamada (antes do 1o yield) o
/// valor eh ignorado pela maquina (nao ha' estado pos-yield que o leia), igual
/// a' spec JS.
#[rtse::abi(native, value = "generator_next_sent")]
pub fn generator_next_sent(h: u64, arg: i64) -> u64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.sent = arg;
        }
    });
    __rtsn_generator_next(h)
}

/// `gen.next()` para generators lazy (Entry::GenState). Invoca a fn de estado
/// (que avanca ate o proximo yield), le o flag `done` e monta `{value, done}`.
/// Se ja' terminado, devolve `{value:undefined, done:true}` sem reinvocar.
#[rtse::abi(native, value = "gen_sm_next")]
pub fn gen_sm_next(h: u64) -> u64 {
    // (cross-runtime #392) async generator: `.next()` devolve Promise<{value,
    // done}>; `await it.next()` desempacota e o for-await aguarda. Sync gen segue.
    //
    // Producing that Promise needs the scheduler, which lives in `rts-std` — see
    // `super::AgenDriver`. With no driver installed the runtime has not booted,
    // and reporting `done` is the only answer that neither blocks nor invents a
    // value.
    if is_async_gen(h) {
        return match agen_driver() {
            Some(d) => (d.next)(h),
            None => make_result(UNDEFINED, true),
        };
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
    // O `sent` pertence a ESTA retomada e so' a ela: `next(v)` o injeta logo
    // antes (`generator_next_sent`), o estado pos-yield o le' via SM_SENT, e
    // aqui ele expira. Sem esta limpeza um `next()` SEM argumento herdava o
    // valor do `next(v)` anterior e um `const a = yield x` posterior lia o
    // valor VELHO em vez de `undefined` — resposta errada em silencio
    // (`[…, {"value":"s:v"}]` onde o Node da `{"value":2}`).
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.sent = SENT_UNDEFINED;
        }
    });
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
#[rtse::abi(native, value = "gen_sm_return")]
pub fn gen_sm_return(h: u64, value: i64) -> u64 {
    gen_sm_abrupt(h, value, 1)
}

/// `gen.throw(e)` para generators lazy (#477 fatia 2). Se ha' uma try-region
/// ativa com `finally`, redireciona para o finally (completion `throw`
/// pendente). O `yield` no finally ABSORVE o throw -> suspende devolvendo
/// `{value:yield, done:false}` SEM propagar a excecao. Sem finally ativo,
/// propaga: marca done e seta o error slot (caller re-lanca).
#[rtse::abi(native, value = "gen_sm_throw")]
pub fn gen_sm_throw(h: u64, err: i64) -> u64 {
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
    crate::collector::error::__rtsn_error_set(err as u64);
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
#[rtse::abi(native, value = "gen_sm_enter_try")]
pub fn gen_sm_enter_try(h: u64, finally_state: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.finally_state = finally_state;
        }
    });
}

/// `__RTS_GEN_SM_ENTER_TRY_CATCH(h, catch_state)` (#211) — registra o estado de
/// entrada do `catch`. `.throw(e)` suspenso dentro da try salta para ele.
#[rtse::abi(native, value = "gen_sm_enter_try_catch")]
pub fn gen_sm_enter_try_catch(h: u64, catch_state: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.catch_state = catch_state;
        }
    });
}

/// `__RTS_GEN_SM_EXIT_TRY_CATCH(h)` (#211) — limpa o catch ativo (saida normal
/// do try body, sem excecao). Um `.throw` posterior nao roteia mais p/ esse
/// catch.
#[rtse::abi(native, value = "gen_sm_exit_try_catch")]
pub fn gen_sm_exit_try_catch(h: u64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.catch_state = -1;
        }
    });
}

/// `__RTS_GEN_SM_CAUGHT(h)` (#211) — valor da excecao capturada (a binding do
/// `catch (e)`). Lido no inicio do estado do catch.
#[rtse::abi(native, value = "gen_sm_caught")]
pub fn gen_sm_caught(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => g.pending_val,
        _ => UNDEFINED,
    })
}

/// `__RTS_GEN_SM_END_FINALLY(h)` — chamado ao fim do bloco finally. Limpa a
/// try-region ativa e honra a completion pendente: para `return`, retorna
/// `DONE(pending_val)`; para `throw`, seta o error slot e marca done. Devolve o
/// valor que a fn de estado deve retornar (e que NEXT empacotara em {value,done}).
#[rtse::abi(native, value = "gen_sm_end_finally")]
pub fn gen_sm_end_finally(h: u64) -> i64 {
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
            crate::collector::error::__rtsn_error_set(val as u64);
            val
        }
        _ => UNDEFINED, // sem completion pendente: caller segue p/ estado normal.
    }
}

/// `__RTS_GEN_SM_DRAIN(h)` — consome o generator lazy ate `done`, coletando os
/// valores yieldados num `Entry::Vec`. Usado por for-of/spread sobre generator
/// state-machine (finito). O valor de `return X` NAO entra (semantica iterador).
#[rtse::abi(native, value = "gen_sm_drain")]
pub fn gen_sm_drain(h: u64) -> u64 {
    // (cross-runtime #392) async generator: drena via AGEN_NEXT (que bombeia
    // alem dos awaits internos e devolve Promise<{value,done}> ja' resolvida).
    // Coleta os valores yieldados num Vec; se o corpo lancar, AGEN_NEXT rejeita
    // -> PROMISE_WAIT seta o error slot -> paramos (o for-of itera o que ja' foi
    // coletado e o try/catch ao redor pega a excecao no fall-through). Sem isto
    // a drena chamaria a state-fn crua e empurraria os dummies dos awaits (0).
    if let (true, Some(driver)) = (is_async_gen(h), agen_driver()) {
        let mut out: Vec<i64> = Vec::new();
        loop {
            let done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            if done {
                break;
            }
            let p = (driver.next)(h);
            let result_map = (driver.await_settled)(p);
            // Throw dentro do corpo: AGEN_NEXT rejeitou -> error slot setado.
            if crate::collector::error::__rtsn_error_get() != 0 {
                break;
            }
            let (val, dn) = read_result_parts(result_map as u64).unwrap_or((UNDEFINED, true));
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

#[rtse::abi(native, value = "gen_sm_is")]
pub fn gen_sm_is(h: u64) -> i64 {
    with_entry(h, |e| matches!(e, Some(Entry::GenState(_))) as i64)
}

