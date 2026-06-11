//! `promise` namespace — primitivas de Promise<T> async (issue #412).
//!
//! Diferente de `Entry::Promise(i64)` (Promise sincrona ja' resolvida —
//! caminho rapido legado de `globals/fetch`), aqui temos
//! `Entry::PromiseAsync(Arc<PromiseSlot>)` com state machine completo:
//! pending/fulfilled/rejected + waiters via tokio oneshot.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).
//! `then`/`catch`/`finally` perderam o sufixo `_NS` no simbolo (agora
//! `__RTS_FN_NS_PROMISE_{THEN,CATCH,FINALLY}`) — interno, distinto do escopo
//! `GL` de `Promise.prototype`, fora de `rts.d.ts`.

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use crate::promise_slot;

use std::sync::atomic::{AtomicUsize, Ordering};

/// (#376) Contador global de tasks tokio pendentes spawnadas por promise.create.
/// Drain do pipeline aguarda chegar a 0 para que async fns fire-and-forget
/// (sem await no top-level) terminem antes do processo sair.
pub(crate) static PENDING_PROMISE_TASKS: AtomicUsize = AtomicUsize::new(0);

/// Bloqueia ate todas as tasks de promise.create completarem, com deadline
/// de 5s para evitar hang em tasks que nunca settle.
pub fn drain_pending_promises() {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    while PENDING_PROMISE_TASKS.load(Ordering::Acquire) > 0 {
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn with_slot<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&std::sync::Arc<rts_engine::heap::handles::PromiseSlot>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => f(arc),
        _ => default,
    })
}

/// Resolve `fp` para ponteiro de codigo executavel.
///
/// Se `fp` for handle de `Entry::Function` (criada via `new Function` ou
/// via reify de user fn), extrai o `fn_ptr` interno + bound_args. Senao,
/// trata como ponteiro extern "C" direto (path legado).
///
/// Retorna `(fn_ptr, bound_args, has_bound_this, bound_this)`.
fn resolve_callback_ptr(fp: u64) -> (u64, Vec<i64>, bool, i64) {
    if fp == 0 {
        return (0, Vec::new(), false, 0);
    }
    let resolved = with_entry(fp, |entry| {
        if let Some(Entry::Function(fd)) = entry {
            Some((
                fd.fn_ptr,
                fd.bound_args.clone(),
                fd.has_bound_this,
                fd.bound_this,
            ))
        } else {
            None
        }
    });
    resolved.unwrap_or((fp, Vec::new(), false, 0))
}

/// Le o Vec<i64> de handles de Promise.
fn collect_promise_handles(vec_handle: u64) -> Vec<u64> {
    with_entry(vec_handle, |entry| match entry {
        Some(Entry::Vec(v)) => v.iter().map(|x| *x as u64).collect(),
        _ => Vec::new(),
    })
}

/// Clone os Arc<PromiseSlot> de cada handle. Handle invalido vira None
/// (filtrado depois).
fn collect_slots(
    handles: &[u64],
) -> Vec<Option<std::sync::Arc<rts_engine::heap::handles::PromiseSlot>>> {
    handles
        .iter()
        .map(|h| {
            with_entry(*h, |entry| match entry {
                Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
                _ => None,
            })
        })
        .collect()
}

/// Variante de invoke_callback que recebe args ja' empacotados (sem extra).
unsafe fn invoke_callback_full(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3],
            ),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4],
            ),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(
                fn_ptr,
            )(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            _ => 0,
        }
    }
}

/// Le o conteudo de um Vec<i64> handle (ou retorna vazio se nao for Vec).
fn read_promise_vec(h: u64) -> Vec<i64> {
    if h == 0 {
        return Vec::new();
    }
    with_entry(h, |entry| {
        if let Some(Entry::Vec(v)) = entry {
            v.iter().copied().collect()
        } else {
            Vec::new()
        }
    })
}

// ── Externs: cada membro `#[rts_fn]` vira um `extern "C"` próprio. ────────────

/// Cria uma Promise async pending. Use `promise.resolve(h, v)` ou `promise.reject(h, e)` depois pra settle. Outras Promises (sync/JS Promise.resolve/reject) usam atalhos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_PENDING() -> Handle {
    let slot = promise_slot::new_pending();
    alloc_entry(Entry::PromiseAsync(slot))
}

/// Cria Promise async ja' fulfilled com `value`. Equivalente do `Promise.resolve(v)` JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_RESOLVED(value: I64) -> Handle {
    let slot = promise_slot::new_fulfilled(value);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// Cria Promise async ja' rejected com `error`. Equivalente do `Promise.reject(e)` JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_REJECTED(error: I64) -> Handle {
    let slot = promise_slot::new_rejected(error);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// Resolve Promise pending com `value`. Retorna 1 em sucesso, 0 se ja' estava settled (semantica JS — segundo resolve eh no-op).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_RESOLVE(promise: U64, value: I64) -> I64 {
    with_slot(promise, 0, |slot| {
        if promise_slot::resolve(slot, value) {
            1
        } else {
            0
        }
    })
}

/// Reject Promise pending com `error`. Retorna 1 em sucesso, 0 se ja' estava settled.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_REJECT(promise: U64, error: I64) -> I64 {
    with_slot(promise, 0, |slot| {
        if promise_slot::reject(slot, error) {
            1
        } else {
            0
        }
    })
}

/// Retorna 0 (pending), 1 (fulfilled) ou 2 (rejected). -1 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_STATE(promise: U64) -> I64 {
    with_slot(promise, -1, |slot| promise_slot::current_state(slot) as i64)
}

/// Bloqueia thread chamadora ate Promise settle e retorna o valor. Se rejected, retorna o erro com bit alto setado (F5 vai tratar isso pra integrar try/catch). 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_WAIT(promise: U64) -> I64 {
    // Clona o Arc fora do `with_entry` pra liberar o lock do shard
    // antes de bloquear em wait_blocking (que pode esperar minutos).
    // Sem isso, qualquer outra op no mesmo shard fica bloqueada.
    let slot_arc = with_entry(promise, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return 0 };
    // (cross-runtime #56/#285/#393) Enquanto a Promise esta PENDING, bombeia o
    // event loop na thread do main: microtasks (fast-path de `.then` settled e
    // chains) + timers vencidos. setTimeout virou queue-based (sem thread por
    // timer), entao `await new Promise(r => setTimeout(r, N))` so' resolve se
    // alguem bombear os timers — e' o await que faz isso aqui. Promises
    // resolvidas por threads tokio (async/promise.create/Promise.all) sao
    // detectadas pelo re-check de estado a cada tick curto.
    if promise_slot::current_state(&arc) == promise_slot::STATE_PENDING {
        use crate::globals::timers::instance as timers;
        use std::time::{Duration, Instant};
        let cap = Instant::now() + Duration::from_secs(5);
        // Bombeia 1x (drena chains de microtask + timers ja' vencidos).
        crate::globals::text_encoding::instance::drain_microtasks();
        timers::pump_due_macrotasks();
        // Enquanto a Promise depende de um timer futuro, avanca o tempo ate o
        // proximo deadline e re-bombeia. Quando nao ha mais timers pendentes,
        // sai do loop e usa wait_blocking (condvar eficiente) p/ resolucao por
        // thread tokio — sem busy-spin.
        while promise_slot::current_state(&arc) == promise_slot::STATE_PENDING {
            let Some(next) = timers::next_macrotask_deadline() else {
                break;
            };
            let now = Instant::now();
            if now >= cap {
                break;
            }
            let wake = next.min(cap);
            if wake > now {
                std::thread::sleep((wake - now).min(Duration::from_millis(50)));
            }
            crate::globals::text_encoding::instance::drain_microtasks();
            timers::pump_due_macrotasks();
        }
    }
    let (state, value) = promise_slot::wait_blocking(&arc);
    // F5 (#416): se Promise rejected, propaga via slot de erro
    // thread-local que `try/catch` ja' le. O slot eh da thread atual
    // (caller do `promise.wait`) — wait_blocking nao migra threads.
    if state == promise_slot::STATE_REJECTED {
        crate::gc_surface::__RTS_FN_RT_ERROR_SET(value as u64);
    }
    value
}

/// Nao-bloqueante: retorna o valor se Promise ja' settled, 0 se ainda pending ou handle invalido. Para checar pending vs settled use `state` antes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_TRY_VALUE(promise: U64) -> I64 {
    with_slot(promise, 0, |slot| {
        if promise_slot::current_state(slot) == promise_slot::STATE_PENDING {
            0
        } else {
            promise_slot::current_value(slot)
        }
    })
}

/// promise.then(p, fn) — chama fn(value) ao resolve, retorna nova Promise resolvida com retorno de fn. Para PromiseAsync, spawna task tokio que aguarda settle. Equivalente a `p.then(fn)` JS mas com sintaxe namespace pra evitar conflito com instance methods do Promise class spec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_THEN(p_handle: U64, fp: U64) -> Handle {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    // Resolve handle Function -> fn_ptr + bound args (ou usa fp como ptr direto).
    let (fn_ptr, bound, _has_this, _this) = resolve_callback_ptr(fp);

    // (cross-runtime #56/#285) Fast-path: se a Promise ja' esta settled,
    // enfileira no microtask queue em vez de spawn_blocking. Isso preserva
    // a ordem JS spec (microtask FIFO) entre queueMicrotask e
    // Promise.resolve().then(). Sem isso, spawn_blocking pode rodar antes
    // do drain do microtask queue.
    let state_now = promise_slot::current_state(&arc);
    if state_now != promise_slot::STATE_PENDING {
        let value = promise_slot::current_value(&arc);
        let fulfilled = state_now == promise_slot::STATE_FULFILLED;
        crate::globals::text_encoding::instance::enqueue_microtask_settled(
            fn_ptr,
            bound,
            value,
            fulfilled,
            result_clone,
        );
        return result_handle;
    }

    // (#207) Source PENDING: enfileira como PendingThen na microtask queue
    // (polling determinista no drain) em vez de spawn_blocking (thread nao-
    // deterministica). Preserva ordem FIFO entre chains de Promise no mesmo
    // task sync — `Promise.resolve().then().then()` interleaving correto.
    // O drain re-checa o estado a cada ciclo; quando settle, executa.
    crate::globals::text_encoding::instance::enqueue_microtask_pending_then(
        arc,
        fn_ptr,
        bound,
        false,
        result_clone,
    );
    result_handle
}

/// promise.catch(p, fn) — chama fn(err) ao reject. Recovers (Promise resultante eh fulfilled).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_CATCH(p_handle: U64, fp: U64) -> Handle {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound, _h, _t) = resolve_callback_ptr(fp);
    // (#207) Determinista via microtask queue (igual ao .then). is_catch=true:
    // o callback so' roda no rejected; fulfilled propaga o valor.
    crate::globals::text_encoding::instance::enqueue_microtask_pending_then(
        arc,
        fn_ptr,
        bound,
        true,
        result_clone,
    );
    result_handle
}

/// promise.finally(p, fn) — chama fn() ao settle. Mantem state/value original.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_FINALLY(p_handle: U64, fp: U64) -> Handle {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, _bound, _h, _t) = resolve_callback_ptr(fp);
    // (#207) Determinista via microtask queue (igual ao .then/.catch).
    crate::globals::text_encoding::instance::enqueue_microtask_pending_finally(
        arc,
        fn_ptr,
        result_clone,
    );
    result_handle
}

/// Le e limpa o slot de erro thread-local. Retorna handle do erro pendente ou 0 se nao houver. Usado internamente pelo codegen de async fn (F5 #416) — apos chamar o body, watcher checa este slot pra decidir entre `resolve` e `reject`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_TAKE_ERROR() -> I64 {
    use crate::gc_surface as error;
    let h = error::__RTS_FN_RT_ERROR_GET();
    if h != 0 {
        error::__RTS_FN_RT_ERROR_CLEAR();
    }
    h as i64
}

/// Promise.all(promises): aguarda todas as Promises do Vec resolverem. Retorna nova Promise resolvida com Vec dos valores na ordem original. Se qualquer uma rejeitar, a Promise resultante rejeita imediatamente com o erro da primeira a rejeitar. Argumento eh handle de `collections.vec` contendo handles de Promise. Equivalente a `Promise.all` JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ALL(promises: U64) -> Handle {
    let handles = collect_promise_handles(promises);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #806) Fast-path: todos settled -> processa sync.
    let all_settled_sync = slots.iter().all(|s| {
        s.as_ref()
            .map(|arc| promise_slot::current_state(arc) != promise_slot::STATE_PENDING)
            .unwrap_or(false)
    });
    if all_settled_sync && !slots.is_empty() {
        let mut values: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let s = slot.as_ref().unwrap();
            let state = promise_slot::current_state(s);
            let value = promise_slot::current_value(s);
            if state == promise_slot::STATE_REJECTED {
                promise_slot::reject(&result, value);
                return result_handle;
            }
            values.push(value);
        }
        let result_vec = alloc_entry(Entry::Vec(Box::new(values)));
        promise_slot::resolve(&result, result_vec as i64);
        return result_handle;
    }
    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let mut values: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let Some(s) = slot else {
                // Handle invalido — rejeita com mensagem fallback.
                let msg = b"Invalid promise handle in collection".to_vec();
                let err_handle = alloc_entry(Entry::String(msg));
                promise_slot::reject(&result_clone, err_handle as i64);
                return;
            };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_REJECTED {
                promise_slot::reject(&result_clone, value);
                return;
            }
            values.push(value);
        }
        // Todos resolveram — empacota num Vec novo.
        let result_vec = alloc_entry(Entry::Vec(Box::new(values)));
        promise_slot::resolve(&result_clone, result_vec as i64);
    });

    result_handle
}

/// Promise.race(promises): retorna nova Promise que settle com o resultado da primeira Promise a settle (resolve OU reject). Equivalente a `Promise.race` JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_RACE(promises: U64) -> Handle {
    let handles = collect_promise_handles(promises);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #779) Fast-path: se algum slot ja' esta settled em
    // ordem de iteracao, resolve sync — preserva FIFO microtask order do
    // JS. Sem isto, spawn_blocking pode reordenar resolves de promises
    // ja' settled (Promise.resolve sync) e quebrar a ordem esperada do
    // event loop.
    for slot in slots.iter() {
        if let Some(s) = slot {
            let state = promise_slot::current_state(s);
            if state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(s);
                if state == promise_slot::STATE_FULFILLED {
                    promise_slot::resolve(&result, value);
                } else {
                    promise_slot::reject(&result, value);
                }
                return result_handle;
            }
        }
    }

    let rt = crate::runtime::async_rt::handle();
    // Cada slot e' aguardado numa task separada — primeira a settle
    // resolve a result. Demais resolves sao no-op (idempotencia).
    for slot in slots {
        let result_clone = result.clone();
        rt.spawn_blocking(move || {
            let Some(s) = slot else {
                promise_slot::reject(&result_clone, 0);
                return;
            };
            let (state, value) = promise_slot::wait_blocking(&s);
            if state == promise_slot::STATE_FULFILLED {
                promise_slot::resolve(&result_clone, value);
            } else {
                promise_slot::reject(&result_clone, value);
            }
        });
    }

    result_handle
}

/// Promise.any(promises): resolve com a primeira a fulfill. Rejeita SO' se todas rejeitarem (com 0 — sem AggregateError ainda). Equivalente a `Promise.any` JS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ANY(promises: U64) -> Handle {
    let handles = collect_promise_handles(promises);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #779) Fast-path: primeira fulfilled em ordem.
    // Mesma motivacao de PROMISE_RACE — preserva FIFO microtask order
    // pra promises ja' settled.
    let mut all_already_rejected = true;
    let mut any_pending = false;
    for slot in slots.iter() {
        if let Some(s) = slot {
            let state = promise_slot::current_state(s);
            if state == promise_slot::STATE_PENDING {
                any_pending = true;
                all_already_rejected = false;
            } else if state == promise_slot::STATE_FULFILLED {
                let value = promise_slot::current_value(s);
                promise_slot::resolve(&result, value);
                return result_handle;
            }
            // rejected — continua
        } else {
            any_pending = true;
            all_already_rejected = false;
        }
    }
    if !any_pending && all_already_rejected {
        let msg = b"All promises were rejected".to_vec();
        let err_handle = alloc_entry(Entry::String(msg));
        promise_slot::reject(&result, err_handle as i64);
        return result_handle;
    }
    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let mut all_rejected = true;
        for slot in slots.iter() {
            let Some(s) = slot else { continue };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_FULFILLED {
                promise_slot::resolve(&result_clone, value);
                return;
            }
            // rejected — registra mas continua tentando proxima
            all_rejected = all_rejected && true;
            let _ = value;
        }
        if all_rejected {
            // Todas rejeitaram — JS daria AggregateError. Aqui usamos
            // handle de string fallback (nao 0, senao slot de erro
            // thread-local nao dispara catch — semantica de "no error"
            // e' value=0).
            let msg = b"All promises were rejected".to_vec();
            let err_handle = alloc_entry(Entry::String(msg));
            promise_slot::reject(&result_clone, err_handle as i64);
        }
    });

    result_handle
}

/// promise.create(fn, args) — cria PromiseAsync executando `fn(...args)` em tokio task. Concentra spawn+state na Promise. `args` eh handle de Vec<i64> ou 0. Settle automatico no retorno (resolve) ou em throw (reject via error slot).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_CREATE(fn_handle: U64, args_vec_handle: U64) -> Handle {
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound, _h, _t) = resolve_callback_ptr(fn_handle);
    if fn_ptr == 0 {
        // fn invalida — Promise rejeitada com 0.
        promise_slot::reject(&result_clone, 0);
        return handle;
    }

    let extra_args = read_promise_vec(args_vec_handle);

    PENDING_PROMISE_TASKS.fetch_add(1, Ordering::AcqRel);
    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        // (cross-runtime #365) marca thread como async-worker: parallel.map
        // dentro do corpo roda sequencial (rayon-em-spawn_blocking crasha).
        let _aw = crate::runtime::async_rt::AsyncWorkerGuard::enter();
        // Combina bound (de bind) + extra_args do caller.
        let mut all: Vec<i64> = bound;
        all.extend(extra_args);
        // Tudo ja' empacotado em `all`, invocamos diretamente.
        let r = unsafe { invoke_callback_full(fn_ptr, &all) };
        // Checa error slot pra detectar throw dentro do body.
        let err = crate::gc_surface::__RTS_FN_RT_ERROR_GET();
        if err != 0 {
            crate::gc_surface::__RTS_FN_RT_ERROR_CLEAR();
            promise_slot::reject(&result_clone, err as i64);
        } else {
            promise_slot::resolve(&result_clone, r);
        }
        PENDING_PROMISE_TASKS.fetch_sub(1, Ordering::AcqRel);
    });
    handle
}

/// Promise.allSettled(promises): aguarda todas, sempre resolve. Retorna Vec onde cada slot eh state*1000 + value (encoding: 1xxx=fulfilled, 2xxx=rejected). Permite caller distinguir valores positivos pequenos. Diferente do JS que retorna {status, value/reason} — RTS usa encoding compacto i64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ALL_SETTLED(promises: U64) -> Handle {
    let handles = collect_promise_handles(promises);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #806) Fast-path: se todos os slots ja' estao settled
    // (Promise.resolve/reject), monta result sync sem spawn. Preserva
    // ordem FIFO de microtasks e garante que .then() em chain rode antes
    // do setTimeout terminar.
    let all_settled_sync = slots.iter().all(|s| {
        s.as_ref()
            .map(|arc| promise_slot::current_state(arc) != promise_slot::STATE_PENDING)
            .unwrap_or(true) // None tambem conta como "settled" (rejected default)
    });
    if all_settled_sync {
        use indexmap::IndexMap;
        let mk_str = |s: &[u8]| -> i64 { alloc_entry(Entry::String(s.to_vec())) as i64 };
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let mut obj: IndexMap<String, i64> = IndexMap::new();
            let Some(s) = slot else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), 0);
                let h = alloc_entry(Entry::Map(Box::new(obj)));
                result_vec.push(h as i64);
                continue;
            };
            let state = promise_slot::current_state(s);
            let value = promise_slot::current_value(s);
            if state == promise_slot::STATE_FULFILLED {
                obj.insert("status".to_string(), mk_str(b"fulfilled"));
                obj.insert("value".to_string(), value);
            } else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), value);
            }
            let h = alloc_entry(Entry::Map(Box::new(obj)));
            result_vec.push(h as i64);
        }
        let result_vec_h = alloc_entry(Entry::Vec(Box::new(result_vec)));
        promise_slot::resolve(&result, result_vec_h as i64);
        return result_handle;
    }

    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        // JS spec: array de { status: "fulfilled", value } | { status: "rejected", reason }.
        // Cada elemento e' um Map (objeto JS) com chaves "status" + "value"/"reason"
        // armazenando handles de string ("fulfilled"/"rejected") e o valor/reason raw.
        use indexmap::IndexMap;
        let mk_str = |s: &[u8]| -> i64 { alloc_entry(Entry::String(s.to_vec())) as i64 };
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let mut obj: IndexMap<String, i64> = IndexMap::new();
            let Some(s) = slot else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), 0);
                let h = alloc_entry(Entry::Map(Box::new(obj)));
                result_vec.push(h as i64);
                continue;
            };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_FULFILLED {
                obj.insert("status".to_string(), mk_str(b"fulfilled"));
                obj.insert("value".to_string(), value);
            } else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), value);
            }
            let h = alloc_entry(Entry::Map(Box::new(obj)));
            result_vec.push(h as i64);
        }
        let result_vec_h = alloc_entry(Entry::Vec(Box::new(result_vec)));
        promise_slot::resolve(&result_clone, result_vec_h as i64);
    });

    result_handle
}

/// Função `promise.f(args)`.
#[allow(clippy::too_many_arguments)]
fn func(
    name: &str,
    symbol: &str,
    sig: Sig,
    flags: MemberFlags,
    fp: *const u8,
    ts: &str,
    doc: &str,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `promise` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("promise")
        .doc("Promise<T> async com state machine + waiters via tokio oneshot. Base para async/await (issue #412 / epic #411).")
        .member(func(
            "new_pending",
            "__RTS_FN_NS_PROMISE_NEW_PENDING",
            Sig::new(vec![], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_NEW_PENDING as *const u8,
            "new_pending(): number",
            "Cria uma Promise async pending. Use `promise.resolve(h, v)` ou `promise.reject(h, e)` depois pra settle. Outras Promises (sync/JS Promise.resolve/reject) usam atalhos.",
        ))
        .member(func(
            "new_resolved",
            "__RTS_FN_NS_PROMISE_NEW_RESOLVED",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_NEW_RESOLVED as *const u8,
            "new_resolved(value: number): number",
            "Cria Promise async ja' fulfilled com `value`. Equivalente do `Promise.resolve(v)` JS.",
        ))
        .member(func(
            "new_rejected",
            "__RTS_FN_NS_PROMISE_NEW_REJECTED",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_NEW_REJECTED as *const u8,
            "new_rejected(error: number): number",
            "Cria Promise async ja' rejected com `error`. Equivalente do `Promise.reject(e)` JS.",
        ))
        .member(func(
            "resolve",
            "__RTS_FN_NS_PROMISE_RESOLVE",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_RESOLVE as *const u8,
            "resolve(promise: number, value: number): number",
            "Resolve Promise pending com `value`. Retorna 1 em sucesso, 0 se ja' estava settled (semantica JS — segundo resolve eh no-op).",
        ))
        .member(func(
            "reject",
            "__RTS_FN_NS_PROMISE_REJECT",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_REJECT as *const u8,
            "reject(promise: number, error: number): number",
            "Reject Promise pending com `error`. Retorna 1 em sucesso, 0 se ja' estava settled.",
        ))
        .member(func(
            "state",
            "__RTS_FN_NS_PROMISE_STATE",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_STATE as *const u8,
            "state(promise: number): number",
            "Retorna 0 (pending), 1 (fulfilled) ou 2 (rejected). -1 se handle invalido.",
        ))
        .member(func(
            "wait",
            "__RTS_FN_NS_PROMISE_WAIT",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            MemberFlags::AMBIGUOUS_RET,
            __RTS_FN_NS_PROMISE_WAIT as *const u8,
            "wait(promise: number): number",
            "Bloqueia thread chamadora ate Promise settle e retorna o valor. Se rejected, retorna o erro com bit alto setado (F5 vai tratar isso pra integrar try/catch). 0 se handle invalido.",
        ))
        .member(func(
            "try_value",
            "__RTS_FN_NS_PROMISE_TRY_VALUE",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_TRY_VALUE as *const u8,
            "try_value(promise: number): number",
            "Nao-bloqueante: retorna o valor se Promise ja' settled, 0 se ainda pending ou handle invalido. Para checar pending vs settled use `state` antes.",
        ))
        .member(func(
            "then",
            "__RTS_FN_NS_PROMISE_THEN",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_THEN as *const u8,
            "then(p: number, fn: (v: number) => number): number",
            "promise.then(p, fn) — chama fn(value) ao resolve, retorna nova Promise resolvida com retorno de fn. Para PromiseAsync, spawna task tokio que aguarda settle. Equivalente a `p.then(fn)` JS mas com sintaxe namespace pra evitar conflito com instance methods do Promise class spec.",
        ))
        .member(func(
            "catch",
            "__RTS_FN_NS_PROMISE_CATCH",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_CATCH as *const u8,
            "catch(p: number, fn: (e: number) => number): number",
            "promise.catch(p, fn) — chama fn(err) ao reject. Recovers (Promise resultante eh fulfilled).",
        ))
        .member(func(
            "finally",
            "__RTS_FN_NS_PROMISE_FINALLY",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_FINALLY as *const u8,
            "finally(p: number, fn: () => void): number",
            "promise.finally(p, fn) — chama fn() ao settle. Mantem state/value original.",
        ))
        .member(func(
            "take_error",
            "__RTS_FN_NS_PROMISE_TAKE_ERROR",
            Sig::new(vec![], AbiType::I64),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_TAKE_ERROR as *const u8,
            "take_error(): number",
            "Le e limpa o slot de erro thread-local. Retorna handle do erro pendente ou 0 se nao houver. Usado internamente pelo codegen de async fn (F5 #416) — apos chamar o body, watcher checa este slot pra decidir entre `resolve` e `reject`.",
        ))
        .member(func(
            "all",
            "__RTS_FN_NS_PROMISE_ALL",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_ALL as *const u8,
            "all(promises: number): number",
            "Promise.all(promises): aguarda todas as Promises do Vec resolverem. Retorna nova Promise resolvida com Vec dos valores na ordem original. Se qualquer uma rejeitar, a Promise resultante rejeita imediatamente com o erro da primeira a rejeitar. Argumento eh handle de `collections.vec` contendo handles de Promise. Equivalente a `Promise.all` JS.",
        ))
        .member(func(
            "race",
            "__RTS_FN_NS_PROMISE_RACE",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_RACE as *const u8,
            "race(promises: number): number",
            "Promise.race(promises): retorna nova Promise que settle com o resultado da primeira Promise a settle (resolve OU reject). Equivalente a `Promise.race` JS.",
        ))
        .member(func(
            "allSettled",
            "__RTS_FN_NS_PROMISE_ALL_SETTLED",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            core::ptr::null::<u8>(),
            "allSettled(promises: number): number",
            "Promise.allSettled(promises): aguarda TODAS settle (nunca rejeita). Resolve com Vec de objetos {status, value} | {status, reason}. Equivalente a `Promise.allSettled` JS.",
        ))
        .member(func(
            "any",
            "__RTS_FN_NS_PROMISE_ANY",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_ANY as *const u8,
            "any(promises: number): number",
            "Promise.any(promises): resolve com a primeira a fulfill. Rejeita SO' se todas rejeitarem (com 0 — sem AggregateError ainda). Equivalente a `Promise.any` JS.",
        ))
        .member(func(
            "create",
            "__RTS_FN_NS_PROMISE_CREATE",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_CREATE as *const u8,
            "create(fn: any, args?: number): number",
            "promise.create(fn, args) — cria PromiseAsync executando `fn(...args)` em tokio task. Concentra spawn+state na Promise. `args` eh handle de Vec<i64> ou 0. Settle automatico no retorno (resolve) ou em throw (reject via error slot).",
        ))
        .member(func(
            "all_settled",
            "__RTS_FN_NS_PROMISE_ALL_SETTLED",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            MemberFlags::NONE,
            __RTS_FN_NS_PROMISE_ALL_SETTLED as *const u8,
            "all_settled(promises: number): number",
            "Promise.allSettled(promises): aguarda todas, sempre resolve. Retorna Vec onde cada slot eh state*1000 + value (encoding: 1xxx=fulfilled, 2xxx=rejected). Permite caller distinguir valores positivos pequenos. Diferente do JS que retorna {status, value/reason} — RTS usa encoding compacto i64.",
        ))
        .done();
}

// ── Non-member externs: codegen calls these by symbol (not in SPECS). ─────────

/// (cross-runtime #392) `await <value>` com passthrough: se `handle` eh uma
/// Promise, bloqueia ate settle e devolve o valor (igual PROMISE_WAIT, propaga
/// reject via error slot); senao devolve o proprio handle inalterado. Usado pelo
/// `for await (const v of arr)` para aguardar CADA elemento (array de Promises),
/// sem quebrar o caso non-Promise (valores ja' resolvidos de async gen drain).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_AWAIT_VALUE(handle: u64) -> i64 {
    let is_promise = with_entry(handle, |e| matches!(e, Some(Entry::PromiseAsync(_))));
    if is_promise {
        __RTS_FN_NS_PROMISE_WAIT(handle)
    } else {
        handle as i64
    }
}

/// `Array.fromAsync(iterable, mapper?)` — coleta valores de iteravel sync ou
/// async para Promise<Array>. Implementacao minima (issue #861):
/// - Vec handle (array sync): aplica mapper a cada elem, wrap em Promise.resolve
///   se nao for Promise ja, depois Promise.all.
/// - Async iterable (gen()): nao suportado ate #211 (generator state machine).
///   Retorna Promise rejeitado.
///
/// `fn_ptr=0` => sem mapper. Caller faz invoke_typed (1 arg, retorna i64/handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM_ASYNC(iterable: u64, mapper_handle: u64) -> u64 {
    // Suporte minimo: iterable e' Vec. Aplica mapper sync (se houver),
    // wrap cada valor em Promise.resolve se ja' nao for, faz Promise.all.
    use rts_shared::globals::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED;

    let snapshot: Option<Vec<i64>> = with_entry(iterable, |e| match e {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    });
    // (cross-runtime #392) async generator handle: drena via GEN_SM_DRAIN (que
    // bombeia os awaits internos e coleta os yields num Vec). Suporta agora
    // `Array.fromAsync(asyncGen())`.
    let snapshot = snapshot.or_else(|| {
        let is_gen = with_entry(iterable, |e| matches!(e, Some(Entry::GenState(_))));
        if !is_gen {
            return None;
        }
        let vec_h = crate::gc_surface::__RTS_FN_NS_GC_GEN_SM_DRAIN(iterable);
        with_entry(vec_h, |e| match e {
            Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
            _ => None,
        })
    });
    let Some(items) = snapshot else {
        // Async iterable / outros tipos — retorna Promise rejected.
        let result = promise_slot::new_pending();
        let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
        let msg =
            b"Array.fromAsync: only sync iterables supported (issue #211 blocks async generators)"
                .to_vec();
        let err_handle = alloc_entry(Entry::String(msg));
        promise_slot::reject(&result, err_handle as i64);
        return result_handle;
    };

    // Aplica mapper se fornecido. mapper(value, index) -> mapped.
    let mapped: Vec<i64> = if mapper_handle != 0 {
        items
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let args_vec = alloc_entry(Entry::Vec(Box::new(vec![v, i as i64])));
                __RTS_FN_GL_FUNCTION_APPLY_TYPED(mapper_handle, 0, args_vec)
            })
            .collect()
    } else {
        items
    };

    // Wrap cada valor: se ja' for Promise handle, mantem; senao Promise.resolve.
    let wrapped: Vec<i64> = mapped
        .iter()
        .map(|&v| {
            let is_promise = with_entry(v as u64, |e| matches!(e, Some(Entry::PromiseAsync(_))));
            if is_promise {
                v
            } else {
                let slot = promise_slot::new_pending();
                promise_slot::resolve(&slot, v);
                alloc_entry(Entry::PromiseAsync(slot)) as i64
            }
        })
        .collect();

    let promises_vec = alloc_entry(Entry::Vec(Box::new(wrapped)));
    __RTS_FN_NS_PROMISE_ALL(promises_vec)
}
