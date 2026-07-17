//! `promise` namespace — primitivas de Promise<T> async (issue #412).
//!
//! Representacao unica: `Entry::PromiseAsync(Arc<PromiseSlot>)` com state
//! machine completo: pending/fulfilled/rejected + waiters via tokio oneshot.
//! (A antiga `Entry::Promise(i64)` sync de `globals/fetch` foi removida.)
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

/// HOOK do slot de erro do MOTOR (rts-adapters `errslot`): o `throw` do motor
/// novo grava a WORD lançada num thread-local PRÓPRIO do codegen, distinto do
/// slot handle legado (`__RTS_FN_RT_ERROR_*`). O watcher de `promise.create`
/// consulta AMBOS ao decidir resolve/reject — sem o hook, um `throw` dentro de
/// uma async fn spawnada resolvia a Promise com o sentinela em vez de rejeitar.
/// `(pending, take)`: `pending() != 0` ⇒ há erro; `take()` lê+limpa a word.
/// Instalado pelo bootstrap do motor (JIT: `compile_program`; AOT: o shim main
/// chama `__rtsadp_engine_bootstrap`). rts-std não pode depender de
/// rts-adapters (direção de deps), daí o registro dinâmico.
#[allow(clippy::type_complexity)]
static ASYNC_ERR_HOOK: std::sync::OnceLock<(extern "C" fn() -> i64, extern "C" fn() -> u64)> =
    std::sync::OnceLock::new();

/// Registra o leitor do slot de erro do motor (chamado pelo rts-adapters).
pub fn set_async_error_hook(pending: extern "C" fn() -> i64, take: extern "C" fn() -> u64) {
    let _ = ASYNC_ERR_HOOK.set((pending, take));
}

/// Lê+limpa o erro pendente do MOTOR nesta thread, se houver: `Some(word)`.
fn take_engine_pending_error() -> Option<u64> {
    let (pending, take) = ASYNC_ERR_HOOK.get()?;
    if pending() != 0 { Some(take()) } else { None }
}

/// O CALLABLE a enfileirar/invocar: um fn VALUE do motor novo (uma WORD
/// `TAG_FUNCTION` ou um handle `Entry::Function` vivo) fica VERBATIM com bound
/// vazio — `invoke_callable_auto` roteia via INVOKE_AUTO (env de uniform-thunk,
/// bound_args, param_kinds). Qualquer outra forma mantém a decomposição legada
/// `resolve_callback_ptr` (fn_ptr cru + bound).
fn callable_or_decomposed(fp: u64) -> (u64, Vec<i64>) {
    let h = rts_engine::heap::poly::poly_handle_normalize(fp).unwrap_or(fp);
    let is_fn_handle = with_entry(h, |e| matches!(e, Some(Entry::Function(_))));
    if is_fn_handle {
        (h, Vec::new())
    } else {
        let (fn_ptr, bound, _has_this, _this) = resolve_callback_ptr(fp);
        (fn_ptr, bound)
    }
}

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

/// (unhandled rejection) Reporta no stderr toda Promise que rejeitou e nunca
/// recebeu handler (.then/.catch/.finally/await/combinador). Chamado pelo
/// `run_event_loop` DEPOIS de todos os drains (quando todo handler que ia anexar
/// já anexou). Formato Node-like; warning não-fatal (não altera exit code, e o
/// stderr não quebra a suite que compara stdout).
pub fn report_unhandled_rejections() {
    for err_h in crate::promise_slot::take_unhandled() {
        eprintln!("Unhandled promise rejection: {}", describe_rejection(err_h as u64));
    }
}

fn describe_rejection(handle: u64) -> String {
    if handle == 0 {
        return "undefined".to_string();
    }
    with_entry(handle, |e| match e {
        Some(Entry::ErrorObj { name, message, .. }) => {
            if message.is_empty() {
                name.clone()
            } else {
                format!("{name}: {message}")
            }
        }
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => format!("{}", handle as i64),
    })
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
        Some(Entry::Vec(v)) => v.iter().map(|x| element_to_handle(*x as u64)).collect(),
        _ => Vec::new(),
    })
}

/// Normalize ONE collection element to a real promise handle, accepting every
/// convention that reaches a `Promise.all/race/any/allSettled` array:
/// - a NaN-boxed `TAG_OBJECT` PolyValue word (the new-engine array element for
///   a promise OBJECT) → its real handle;
/// - a NaN-boxed INT32 / an inline-f64 NUMBER word (a promise handle riding the
///   `promise.*` i64 surface, stored into the array as a plain number) → the
///   integer it encodes;
/// - anything else (a raw handle from the low-level `collections.vec` path) →
///   verbatim. An element that is not actually a promise still becomes a `None`
///   slot downstream (the per-op invalid-element path), same as before.
fn element_to_handle(w: u64) -> u64 {
    if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(w) {
        return h;
    }
    const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
    if (w & BOX_BASE) == BOX_BASE {
        // Boxed non-handle: an INT32 payload is the handle-as-number form; any
        // other tag (singleton/…) is not a promise — 0 keeps the invalid path.
        let tag = (w >> 48) & 0x7;
        if tag == 1 {
            return (w & 0xFFFF_FFFF) as u32 as u64;
        }
        return 0;
    }
    // An inline f64 word encoding a non-negative integer (a handle > 2^31 —
    // generation bits — stored as a plain number): decode it. A RAW handle's
    // bit-pattern reads as a denormal/fractional f64 and falls through verbatim.
    let as_f = f64::from_bits(w);
    if as_f.is_finite() && as_f >= 1.0 && as_f.fract() == 0.0 && as_f < (1u64 << 53) as f64 {
        return as_f as u64;
    }
    w
}

/// Um combinator (all/allSettled/race/any) É um handler dos seus inputs — em
/// JS ele anexa reactions em cada promise passada. Marca cada slot como
/// handled para o report pós-main não acusar "Unhandled promise rejection"
/// numa rejection que o combinator consumiu (os fast-paths leem
/// `current_state`/`current_value` sem passar pelo `wait_blocking`, que era
/// o único ponto que marcava).
fn mark_inputs_handled(slots: &[Option<std::sync::Arc<rts_engine::heap::handles::PromiseSlot>>]) {
    for s in slots.iter().flatten() {
        promise_slot::mark_handled(s);
    }
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
    // Promise/A+: `Promise.resolve(thenable)` ADOPTS the thenable instead of
    // fulfilling with it - start pending and assimilate; a plain value keeps
    // the already-fulfilled fast path.
    if promise_slot::thenable_then_of(value) != 0 {
        let slot = promise_slot::new_pending();
        let h = alloc_entry(Entry::PromiseAsync(slot.clone()));
        resolve_assimilating(&slot, h, value);
        return h;
    }
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

/// Bloqueia thread chamadora ate Promise settle e retorna o valor NORMALIZADO
/// para a superfície i64 (`promise.wait` do TS): uma WORD PolyValue settled por
/// um produtor do motor novo decodifica para o inteiro/handle que representa
/// (um número imprime como número, não como os bits). O `await` do motor NÃO
/// passa por aqui — usa [`wait_raw`] (verbatim) e reboxa a word ele mesmo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_WAIT(promise: U64) -> I64 {
    normalize_settled_i64(wait_raw(promise))
}

/// Normaliza um valor settled cru para a superfície i64: word `TAG_*`-handle →
/// handle real; word INT32 → o inteiro; word SINGLETON (undefined/null) → 0;
/// word f64 inteira → o inteiro; qualquer outra forma passa VERBATIM — em
/// particular um i64 cru NEGATIVO legado, cujos bits caem no quadrante do
/// NaN-box (tag 7) e NÃO podem ser tratados como word (`Promise.resolve(-42)`
/// zerava sem esta distinção).
fn normalize_settled_i64(raw: i64) -> i64 {
    let w = raw as u64;
    if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(w) {
        return h as i64;
    }
    const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
    if (w & BOX_BASE) == BOX_BASE {
        let tag = (w >> 48) & 0x7;
        if tag == 1 {
            return (w & 0xFFFF_FFFF) as u32 as i32 as i64;
        }
        if tag == 2 {
            // singleton (undefined/null) — a superfície i64 lê 0.
            return 0;
        }
        // tag 3/5 já saíram pelo poly_handle_normalize; o resto do quadrante é
        // um i64 cru NEGATIVO legado — verbatim.
        return raw;
    }
    let f = f64::from_bits(w);
    if f.is_finite() && f.fract() == 0.0 && f.abs() >= 1.0 && f.abs() < 9.007e15 {
        return f as i64;
    }
    raw
}

/// O valor settled VERBATIM (a word do motor novo OU o i64 cru legado), com o
/// mesmo bombeio de event-loop do `WAIT` público. Consumido pelo `await` do
/// motor (`__rtsadp_await`), que faz o próprio rebox — normalizar aqui
/// arredondaria um double fracionário.
pub fn wait_raw(promise: u64) -> i64 {
    // Clona o Arc fora do `with_entry` pra liberar o lock do shard
    // antes de bloquear em wait_blocking (que pode esperar minutos).
    // Sem isso, qualquer outra op no mesmo shard fica bloqueada.
    let slot_arc = with_entry(promise, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    // `wait` de um valor que NAO e' uma Promise viva = o proprio valor (semantica
    // JS de `await` sobre nao-thenable). No motor novo as async fns rodam SINCRONO
    // e retornam o valor direto, entao `promise.wait(asyncFn())` recebe o valor
    // ja' resolvido — devolve-lo e' o comportamento correto (era `return 0`).
    let Some(arc) = slot_arc else {
        return promise as i64;
    };
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
    // (.then anexa handler) source consome/encaminha a rejection — não é
    // unhandled; a responsabilidade passa pra `result` (que vira a rejection
    // pendente se não tiver .catch).
    promise_slot::mark_handled(&arc);
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    // Um fn VALUE do motor novo (word/handle Entry::Function) fica VERBATIM como
    // callable — o invocador (`invoke_callable_auto`) aplica env/bound/kinds via
    // INVOKE_AUTO; decompor em fn_ptr+bound aqui perdia o env do uniform-thunk
    // (callback recebia o valor no slot errado). Um fp cru mantém a decomposição.
    let (fn_ptr, bound) = callable_or_decomposed(fp);

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
            promise_slot::value_is_word(&arc),
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
    // (.catch consome a rejection) marca handled.
    promise_slot::mark_handled(&arc);
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound) = callable_or_decomposed(fp);
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
    // (.finally anexa handler) marca handled — encaminha o settlement.
    promise_slot::mark_handled(&arc);
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, _bound) = callable_or_decomposed(fp);
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
    mark_inputs_handled(&slots);
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
            // Normaliza para a superfície i64 do Vec de resultados (uma WORD
            // de produtor novo-motor decodifica; cru legado passa verbatim).
            values.push(normalize_settled_i64(value));
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
            // Normaliza para a superfície i64 do Vec de resultados (uma WORD
            // de produtor novo-motor decodifica; cru legado passa verbatim).
            values.push(normalize_settled_i64(value));
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
    mark_inputs_handled(&slots);
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
    mark_inputs_handled(&slots);
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

    // Explicit `promise.create(fn, [args])` from source: the args array comes
    // from the NEW engine — its elements are PolyValue WORDS.
    create_spawn(fn_handle, args_vec_handle, result_clone, handle, None, true)
}

/// O spawn de `promise.create`, com um `boxer` OPCIONAL aplicado ao valor de
/// RESOLVE: o async-spawn do MOTOR NOVO (`__rtsadp_promise_spawn`) instala um
/// boxer que converte o retorno cru do invoke na WORD PolyValue correta (um
/// i64 NEGATIVO cru colide com o quadrante NaN-box — só o produtor sabe a
/// convenção). Rejeições (ambos os slots de erro) idênticas nos dois modos.
pub fn create_spawn_boxed(
    fn_handle: u64,
    args_vec_handle: u64,
    boxer: extern "C" fn(u64, i64) -> i64,
) -> u64 {
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let handle = alloc_entry(Entry::PromiseAsync(result));
    // The engine's async-CALL spawn (`__rtsadp_promise_spawn`): the args ride
    // the FN_KINDS convention (an f64 travels as its BITS) — NOT words; the
    // registry-typed invoke reads them back. Never word-decode these.
    create_spawn(fn_handle, args_vec_handle, result_clone, handle, Some(boxer), false)
}

fn create_spawn(
    fn_handle: u64,
    args_vec_handle: u64,
    result_clone: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    handle: u64,
    boxer: Option<extern "C" fn(u64, i64) -> i64>,
    args_are_words: bool,
) -> u64 {
    let (fn_ptr, bound) = callable_or_decomposed(fn_handle);
    if fn_ptr == 0 {
        // fn invalida — Promise rejeitada com 0.
        promise_slot::reject(&result_clone, 0);
        return handle;
    }

    let extra_args = read_promise_vec(args_vec_handle);

    PENDING_PROMISE_TASKS.fetch_add(1, Ordering::AcqRel);
    // Os GCELLS (module-globals promovidos) são THREAD-LOCAL: o corpo async
    // roda numa thread tokio e, sem o snapshot/restore (o mesmo que
    // `thread.spawn` faz), lia TODO módulo-global/singleton como 0/undefined
    // (`const b = new Box(); async fn { b.v }` → undefined).
    let gcells = crate::collector::collector::gcell_snapshot();
    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        crate::collector::collector::gcell_restore(gcells);
        // (cross-runtime #365) marca thread como async-worker: parallel.map
        // dentro do corpo roda sequencial (rayon-em-spawn_blocking crasha).
        let _aw = crate::runtime::async_rt::AsyncWorkerGuard::enter();
        // Combina bound (de bind) + extra_args do caller.
        let mut all: Vec<i64> = bound;
        all.extend(extra_args);
        // Ponte de convenção COMPLETA: o args array de um `promise.create`
        // explícito vem do MOTOR NOVO (`args_are_words` — elementos são WORDS
        // PolyValue): um callee uniform-thunk as recebe verbatim e devolve
        // WORD (re-crua p/ o slot i64); um fn_ptr cru/`new Function` recebe os
        // valores DECODIFICADOS (INT32-word 50 → 50). O async-spawn do motor
        // (`create_spawn_boxed`) fala a convenção FN_KINDS (bits f64) — os
        // args passam verbatim pro invoke tipado do registry.
        let r = rts_primitives::function::ops::invoke_callable_bridged(fn_ptr, &all, args_are_words);
        // Checa AMBOS os slots de erro pra detectar throw dentro do body: o
        // handle-slot legado E o errslot do motor novo (via hook — um `throw`
        // novo-motor grava a WORD lançada lá, não aqui).
        let err = crate::gc_surface::__RTS_FN_RT_ERROR_GET();
        if err != 0 {
            crate::gc_surface::__RTS_FN_RT_ERROR_CLEAR();
            promise_slot::reject(&result_clone, err as i64);
        } else if let Some(word) = take_engine_pending_error() {
            promise_slot::reject(&result_clone, word as i64);
        } else {
            let boxed = boxer.is_some();
            let r = match boxer {
                Some(b) => b(fn_ptr, r),
                None => r,
            };
            // Promise/A+: an async body returning a thenable/promise ADOPTS it.
            // Com boxer, `r` já é WORD (box_async_result) — o slot registra a
            // proveniência p/ a entrega ao then não re-adivinhar o tipo.
            resolve_assimilating_word(&result_clone, handle, r, boxed);
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
    mark_inputs_handled(&slots);
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
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let row = match slot {
                None => allsettled_row(promise_slot::STATE_REJECTED, 0),
                Some(s) => {
                    let state = promise_slot::current_state(s);
                    let value = promise_slot::current_value(s);
                    allsettled_row(state, value)
                }
            };
            result_vec.push(row);
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
        // Cada linha e' um objeto SHAPED do motor novo (slots = PolyValue words).
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let row = match slot {
                None => allsettled_row(promise_slot::STATE_REJECTED, 0),
                Some(s) => {
                    let (state, value) = promise_slot::wait_blocking(s);
                    allsettled_row(state, value)
                }
            };
            result_vec.push(row);
        }
        let result_vec_h = alloc_entry(Entry::Vec(Box::new(result_vec)));
        promise_slot::resolve(&result_clone, result_vec_h as i64);
    });

    result_handle
}

/// Linha do `Promise.allSettled` como objeto SHAPED (motor novo):
/// `{ status: "fulfilled", value }` ou `{ status: "rejected", reason }` —
/// retorna o OBJECT word do row (elemento direto do Vec de resultados).
fn allsettled_row(state: u8, value: i64) -> i64 {
    use rts_engine::heap::shapes::{
        alloc_shaped_object, handle_word_auto, legacy_i64_to_word, string_word,
    };
    let fulfilled = state == promise_slot::STATE_FULFILLED;
    let (status, key) = if fulfilled {
        (string_word(b"fulfilled"), "value")
    } else {
        (string_word(b"rejected"), "reason")
    };
    let h = alloc_shaped_object(
        &["status", key],
        &[status as i64, legacy_i64_to_word(value) as i64],
    );
    handle_word_auto(h) as i64
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
    use rts_primitives::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED;

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

// ===========================================================================
// THENABLE assimilation (Promise/A+): fulfilling a promise with a value that
// carries a callable `.then` must ADOPT that thenable's state instead —
// `then(resolveFn, rejectFn)` is invoked ONCE and the first settle wins (the
// slot's first-wins `settle` covers the single-resolution rule).
// ===========================================================================

/// Uniform-ABI settle thunk (RESOLVE) the thenable's `then` invokes: the
/// promise handle rides `env` (bound_this); `a0` is the settle value
/// (PolyValue word ou i64 raw — normalizado downstream por rebox_settled).
extern "C" fn thenable_resolve_thunk(
    env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _rest: u64,
) -> u64 {
    let arc = with_entry(env, |e| match e {
        Some(Entry::PromiseAsync(a)) => Some(a.clone()),
        _ => None,
    });
    if let Some(arc) = arc {
        // Recursive adoption: the resolution value may itself be a thenable.
        resolve_assimilating(&arc, env, a0 as i64);
    }
    rts_engine::heap::poly::POLY_UNDEFINED
}

/// Uniform-ABI settle thunk (REJECT) — espelho do resolve.
extern "C" fn thenable_reject_thunk(
    env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _rest: u64,
) -> u64 {
    let arc = with_entry(env, |e| match e {
        Some(Entry::PromiseAsync(a)) => Some(a.clone()),
        _ => None,
    });
    if let Some(arc) = arc {
        promise_slot::reject(&arc, a0 as i64);
    }
    rts_engine::heap::poly::POLY_UNDEFINED
}

/// Build the settle callable (uniform thunk, promise handle no env) as a
/// function WORD the user's `then` body can invoke.
fn settle_callable_word(promise_h: u64, is_reject: i64) -> i64 {
    use rts_engine::heap::handles::FunctionData;
    let fn_ptr = if is_reject != 0 {
        thenable_reject_thunk as *const () as usize as u64
    } else {
        thenable_resolve_thunk as *const () as usize as u64
    };
    let h = alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity: 1,
        name: Box::from(""),
        bound_this: promise_h as i64,
        has_bound_this: true,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    })));
    // TAG_FUNCTION word over the bare slot (the invoke bridges normalize).
    const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
    let slot = rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
    (BOX_BASE | (5u64 << 48) | slot) as i64
}

/// Fulfil `arc` with `value`, ADOPTING a thenable value instead of storing it:
/// when the probe finds a callable `.then`, invoke it with the settle pair and
/// leave the slot pending until one of them fires. `promise_h` is the live
/// `Entry::PromiseAsync` handle for `arc` (the bridge finds the slot by it).
pub(crate) fn resolve_assimilating(
    arc: &std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    promise_h: u64,
    value: i64,
) {
    resolve_assimilating_word(arc, promise_h, value, false)
}

/// Variante com a PROVENIÊNCIA do valor: `value_is_word = true` quando `value`
/// já é WORD PolyValue (o settle do async-spawn boxa via `box_async_result`) —
/// o slot marca isso e a entrega ao callback de then/catch/finally NÃO
/// re-adivinha o tipo pela ponte raw (que corrompia words de double inline).
pub(crate) fn resolve_assimilating_word(
    arc: &std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    promise_h: u64,
    value: i64,
    value_is_word: bool,
) {
    // A PROMISE value (an async callback returning `Promise.resolve(x)` /
    // another chain): ADOPT its state directly — fulfil/reject through, or
    // chain a pending source via the microtask queue (fn_ptr 0 = pass-through).
    if let Some(src) = with_slot(
        rts_engine::heap::poly::poly_handle_normalize(value as u64).unwrap_or(value as u64),
        None,
        |a| Some(a.clone()),
    ) {
        promise_slot::mark_handled(&src);
        match promise_slot::current_state(&src) {
            promise_slot::STATE_FULFILLED => {
                let inner = promise_slot::current_value(&src);
                resolve_assimilating_word(arc, promise_h, inner, promise_slot::value_is_word(&src));
            }
            promise_slot::STATE_REJECTED => {
                promise_slot::reject(arc, promise_slot::current_value(&src));
            }
            _ => {
                crate::globals::text_encoding::instance::enqueue_microtask_pending_then(
                    src,
                    0,
                    Vec::new(),
                    false,
                    arc.clone(),
                );
            }
        }
        return;
    }
    let then_w = promise_slot::thenable_then_of(value);
    if then_w == 0 {
        if value_is_word {
            promise_slot::resolve_word(arc, value);
        } else {
            promise_slot::resolve(arc, value);
        }
        return;
    }
    let res_w = settle_callable_word(promise_h, 0);
    let rej_w = settle_callable_word(promise_h, 1);
    // An object-literal METHOD materializes as a THIS-FIRST uniform thunk (its
    // a0 slot is the receiver): prepend the thenable's own object word so
    // `resolve`/`reject` land in the right params. A plain fn stays unshifted.
    let real = rts_engine::heap::poly::poly_handle_normalize(then_w).unwrap_or(then_w);
    let has_this = with_entry(real, |e| {
        matches!(e, Some(Entry::Function(d)) if d.has_this_param)
    });
    let args: Vec<i64> = if has_this {
        const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
        let w = value as u64;
        let obj_word = if (w & BOX_BASE) == BOX_BASE {
            value
        } else {
            let slot = rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(w);
            (BOX_BASE | (4u64 << 48) | slot) as i64
        };
        vec![obj_word, res_w, rej_w]
    } else {
        vec![res_w, rej_w]
    };
    // `then` may throw synchronously → reject (spec). The engine's pending-
    // error slot carries the thrown word.
    rts_primitives::function::ops::invoke_callable_auto(then_w, &args);
    if let Some(word) = take_engine_pending_error() {
        promise_slot::reject(arc, word as i64);
    }
}

/// [`resolve_assimilating`] without a pre-existing handle: allocates a fresh
/// `Entry::PromiseAsync` alias for the slot only when the value IS a thenable
/// (the common plain-value path stays allocation-free).
pub(crate) fn resolve_adopting(
    arc: &std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    value: i64,
) {
    // A PROMISE or a thenable value both adopt (the assimilating path handles
    // either); a plain value fulfils allocation-free.
    let is_promise = with_slot(
        rts_engine::heap::poly::poly_handle_normalize(value as u64).unwrap_or(value as u64),
        false,
        |_| true,
    );
    if !is_promise && promise_slot::thenable_then_of(value) == 0 {
        promise_slot::resolve(arc, value);
        return;
    }
    let h = alloc_entry(Entry::PromiseAsync(arc.clone()));
    resolve_assimilating(arc, h, value);
}
