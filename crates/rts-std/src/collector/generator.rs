//! The ASYNC driver for the generator state machine — `async function` and
//! `async function*` (#207 fatia 1, #392).
//!
//! ## Why this half stayed in `rts-std`
//!
//! `RTS_ORGANIZATION.md` N3 planned to move this whole file into `rts-natives`
//! as machinery. Measured, only part of it is: the state machine itself —
//! suspending and resuming a frame, which the Cranelift IR cannot express — is
//! now `rts_engine::collector::generator`. What is left here is the part that
//! DRIVES it, and driving means scheduling: polling timers
//! (`globals::timers`), entering a tokio worker guard
//! (`runtime::async_rt::AsyncWorkerGuard`), enqueuing microtasks and waiting on
//! promise slots. That is backend by every definition in the partition rule, and
//! moving it would have dragged five `rts-std` modules down with it.
//!
//! The line is not "sync vs async": a state machine is a data structure with a
//! `step`, while deciding WHEN to step it is a scheduler.
//!
//! ## The seam
//!
//! `gen_sm_next`/`gen_sm_drain` (below in `rts-natives`) need an async
//! generator's `.next()`, which returns a Promise. They reach it through
//! `rts_engine::collector::generator::AgenDriver`, installed by
//! [`install_agen_driver`] at startup — the same shape as the GC's
//! `root_sources`: the lower layer owns the mechanism, the upper layer
//! contributes the part that needs a scheduler.

use rts_engine::heap::handles::{Entry, GenStateData, alloc_entry, with_entry, with_entry_mut};

use rts_engine::collector::generator::AgenDriver;

/// The state machine itself, re-exported verbatim so that every
/// `rts_std::collector::generator::…` path above this crate keeps resolving —
/// `rts-runtime`'s value model alone reaches for `open_vec_iterator`,
/// `vec_is_open_iterator`, `__rtsn_generator_next/return/throw` and
/// `__rtsm_global_Array_iterator_fn` by that path. A split must be invisible to
/// its consumers, exactly as N1's heap move was.
pub use rts_engine::collector::generator::*;

/// Sentinel `undefined` (i64::MIN+2) — convencao do codegen/INSPECT.
const UNDEFINED: i64 = i64::MIN + 2;

/// Hand the state machine its scheduler. Called once from `runtime_init`.
pub fn install_agen_driver() {
    rts_engine::collector::generator::set_agen_driver(AgenDriver {
        next: |h| __rtsn_agen_next(h),
        await_settled: |p| crate::promise::__rtsm_promise_wait(p),
    });
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
#[rtse::abi(native, value = "async_sm_new")]
pub fn async_sm_new(fn_ptr: u64, nslots: i64) -> u64 {
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
#[rtse::abi(native, value = "agen_new")]
pub fn agen_new(fn_ptr: u64, nslots: i64) -> u64 {
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
#[rtse::abi(native, value = "agen_next")]
pub fn agen_next(h: u64) -> u64 {
    use crate::promise_slot;
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
        use crate::globals::timers::instance as timers;
        let cap = Instant::now() + Duration::from_secs(5);
        while promise_slot::current_state(&result) == promise_slot::STATE_PENDING {
            crate::globals::text_encoding::instance::drain_microtasks();
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
    use crate::promise_slot;
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
        crate::globals::text_encoding::instance::enqueue_microtask_async_resume(h, src);
        return;
    }
    if let Some(np) = np {
        let err = crate::collector::error::__rtsn_error_get();
        if err != 0 {
            crate::collector::error::__rtsn_error_clear();
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
#[rtse::abi(native, value = "async_sm_start")]
pub fn async_sm_start(h: u64) -> u64 {
    use crate::promise_slot;
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
    use crate::promise_slot;
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
            let err = crate::collector::error::__rtsn_error_get();
            if err != 0 {
                crate::collector::error::__rtsn_error_clear();
                promise_slot::reject(&rp, err as i64);
            } else {
                promise_slot::resolve(&rp, ret);
            }
        }
        return;
    }
    // Suspendeu em await: enfileira AsyncResume sobre a promise awaited.
    if let Some(src) = pending {
        crate::globals::text_encoding::instance::enqueue_microtask_async_resume(h, src);
    }
}

/// `__RTS_GEN_SM_ASYNC_SUSPEND(h, promise_handle)` — chamado pela state-fn ao
/// atingir `await x`. Extrai o Arc<PromiseSlot> do handle (ou cria um ja settled
/// se o valor awaited NAO eh uma Promise) e guarda em pending_await. Devolve um
/// valor dummy (a state-fn ja vai retornar logo apos).
#[rtse::abi(native, value = "async_sm_suspend")]
pub fn async_sm_suspend(h: u64, promise_handle: i64) -> i64 {
    use crate::promise_slot;
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
#[rtse::abi(native, value = "async_sm_awaited")]
pub fn async_sm_awaited(h: u64) -> i64 {
    let (val, rejected) = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => (g.awaited_val, g.awaited_rejected),
        _ => (UNDEFINED, false),
    });
    if rejected {
        crate::collector::error::__rtsn_error_set(val as u64);
    }
    val
}

/// `__RTS_GEN_SM_ASYNC_RESOLVE(h, val)` — chamado pela state-fn no `return val`
/// (ou fim do corpo). Marca done e guarda ret; o async_sm_step settla a
/// result_promise apos a state-fn retornar.
#[rtse::abi(native, value = "async_sm_resolve")]
pub fn async_sm_resolve(h: u64, val: i64) -> i64 {
    with_entry_mut(h, |e| {
        if let Some(Entry::GenState(g)) = e {
            g.done = true;
            g.ret = val;
        }
    });
    val
}

/// Wrapper `extern "C"` p/ o drain de microtasks (`text_encoding`) que vive no
/// rts-std chamar este resume sem depender do rts-runtime (quebraria o ciclo
/// std→runtime). `rejected` passa como i64 (0/1). Resolve por link.
#[rtse::abi(native, value = "async_sm_resume")]
pub fn async_sm_resume(h: u64, value: i64, rejected: i64) {
    async_sm_resume(h, value, rejected != 0);
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

