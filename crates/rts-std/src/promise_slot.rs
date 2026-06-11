//! Logica completa de `PromiseSlot` (issue #412 / epic #411).
//!
//! O struct `PromiseSlot` e' declarado em `handles.rs` (que faz parte do
//! `runtime_support` standalone) com `waiters` como `Box<dyn Any>` para
//! nao depender de tokio. Aqui (main crate) implementamos a logica que
//! usa tokio: criacao, resolve/reject, wait_blocking.
//!
//! Convencao para o `value` armazenado:
//! - Fulfilled → handle ou primitivo i64 (depende de Promise<T>)
//! - Rejected → handle de string (mensagem) ou Error
//! - Pending → 0 (irrelevante, checar state primeiro)

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use tokio::sync::oneshot;

use rts_engine::heap::handles::PromiseSlot;

/// State numerico do PromiseSlot.
pub const STATE_PENDING: u8 = 0;
pub const STATE_FULFILLED: u8 = 1;
pub const STATE_REJECTED: u8 = 2;

/// (unhandled rejection) Registro global `slot_ptr -> error_handle` das
/// rejections ainda SEM handler. `reject`/`new_rejected` inserem quando o slot
/// está `handled == false`; `mark_handled` seta o flag + remove; o report no fim
/// do event loop (`take_unhandled`) drena o que sobrou. Chave = endereço do slot
/// (estável enquanto o Arc vive — os slots vivem até o fim do programa via
/// handles PromiseAsync).
static UNHANDLED: OnceLock<Mutex<HashMap<usize, i64>>> = OnceLock::new();

fn unhandled_map() -> &'static Mutex<HashMap<usize, i64>> {
    UNHANDLED.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline]
fn slot_key(slot: &PromiseSlot) -> usize {
    slot as *const PromiseSlot as usize
}

/// Marca a promise como tendo handler anexado (.then/.catch/.finally/await/
/// combinador). Idempotente. Remove de UNHANDLED se já estava registrada.
pub fn mark_handled(slot: &PromiseSlot) {
    slot.handled.store(true, Ordering::Release);
    unhandled_map()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&slot_key(slot));
}

/// Drena os error-handles das rejections que continuam sem handler.
pub fn take_unhandled() -> Vec<i64> {
    let mut map = unhandled_map().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *map).into_values().collect()
}

/// Registra rejection sem handler (chamado por reject/new_rejected). Só registra
/// se o slot ainda não foi marcado handled (evita falso-positivo no caso
/// attach-then-reject: o handler já setou o flag antes da rejection settlar).
fn register_unhandled(slot: &PromiseSlot, error: i64) {
    if slot.handled.load(Ordering::Acquire) {
        return;
    }
    unhandled_map()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(slot_key(slot), error);
}

type WaiterVec = Vec<oneshot::Sender<(u8, i64)>>;

/// Helper: pega `&mut WaiterVec` do `Box<dyn Any>` em PromiseSlot.
fn waiters_mut(slot: &PromiseSlot) -> std::sync::MutexGuard<'_, Box<dyn std::any::Any + Send + Sync>> {
    slot.waiters.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cria slot pending.
pub fn new_pending() -> Arc<PromiseSlot> {
    let waiters: WaiterVec = Vec::new();
    Arc::new(PromiseSlot {
        state: AtomicU8::new(STATE_PENDING),
        value: std::sync::Mutex::new(0),
        waiters: std::sync::Mutex::new(Box::new(waiters)),
        handled: AtomicBool::new(false),
    })
}

/// Cria slot ja' fulfilled. Sem waiters.
pub fn new_fulfilled(value: i64) -> Arc<PromiseSlot> {
    let waiters: WaiterVec = Vec::new();
    Arc::new(PromiseSlot {
        state: AtomicU8::new(STATE_FULFILLED),
        value: std::sync::Mutex::new(value),
        waiters: std::sync::Mutex::new(Box::new(waiters)),
        handled: AtomicBool::new(false),
    })
}

/// Cria slot ja' rejected. Registra como unhandled até um handler anexar.
pub fn new_rejected(error: i64) -> Arc<PromiseSlot> {
    let waiters: WaiterVec = Vec::new();
    let slot = Arc::new(PromiseSlot {
        state: AtomicU8::new(STATE_REJECTED),
        value: std::sync::Mutex::new(error),
        waiters: std::sync::Mutex::new(Box::new(waiters)),
        handled: AtomicBool::new(false),
    });
    register_unhandled(&slot, error);
    slot
}

/// Tenta transitar pending → settled. Retorna true em sucesso.
/// Idempotente em chamadas posteriores (semantica JS Promise).
fn settle(slot: &PromiseSlot, new_state: u8, value: i64) -> bool {
    let prev = slot.state.compare_exchange(
        STATE_PENDING,
        new_state,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    if prev.is_err() {
        return false;
    }
    *slot.value.lock().unwrap_or_else(|e| e.into_inner()) = value;
    // Acorda waiters.
    let mut w_guard = waiters_mut(slot);
    let waiters: &mut WaiterVec = match w_guard.downcast_mut() {
        Some(v) => v,
        None => return true, // tipo errado — improvavel
    };
    let drained = std::mem::take(waiters);
    drop(w_guard);
    for tx in drained {
        let _ = tx.send((new_state, value));
    }
    true
}

pub fn resolve(slot: &PromiseSlot, value: i64) -> bool {
    settle(slot, STATE_FULFILLED, value)
}

pub fn reject(slot: &PromiseSlot, error: i64) -> bool {
    let ok = settle(slot, STATE_REJECTED, error);
    if ok {
        register_unhandled(slot, error);
    }
    ok
}

/// Le state atual.
pub fn current_state(slot: &PromiseSlot) -> u8 {
    slot.state.load(Ordering::Acquire)
}

/// Le valor atual (so' faz sentido apos checar state != pending).
pub fn current_value(slot: &PromiseSlot) -> i64 {
    *slot.value.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bloqueia ate settle. Retorna (state, value).
/// Caso ja' settled, retorna imediatamente sem alocar waiter.
pub fn wait_blocking(slot: &Arc<PromiseSlot>) -> (u8, i64) {
    // await/combinador = handler: consome a rejection, não é unhandled.
    mark_handled(slot);
    let cur = current_state(slot);
    if cur != STATE_PENDING {
        return (cur, current_value(slot));
    }

    let (tx, rx) = oneshot::channel();
    {
        let mut w_guard = waiters_mut(slot);
        // Re-check apos lock (settle pode ter rodado entre load e lock).
        let cur2 = current_state(slot);
        if cur2 != STATE_PENDING {
            return (cur2, current_value(slot));
        }
        let waiters: &mut WaiterVec = match w_guard.downcast_mut() {
            Some(v) => v,
            None => return (STATE_REJECTED, 0),
        };
        waiters.push(tx);
    }

    // (engine multi-thread passivo) Detecta se estamos dentro de uma
    // tokio worker thread (callback de `.then`, `spawn_blocking`, etc).
    // `rt.block_on(rx)` panica "Cannot start a runtime from within a
    // runtime" nesse caso. Usar `block_in_place` para informar ao
    // scheduler tokio que vamos bloquear, depois bloqueia via
    // `Handle::block_on` da runtime atual.
    if crate::runtime::async_rt::in_tokio_thread() {
        return tokio::task::block_in_place(|| {
            let handle = tokio::runtime::Handle::current();
            match handle.block_on(rx) {
                Ok(v) => v,
                Err(_) => (STATE_REJECTED, 0),
            }
        });
    }
    let rt = crate::runtime::async_rt::rt();
    match rt.block_on(rx) {
        Ok(v) => v,
        Err(_) => (STATE_REJECTED, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_fulfilled_returns_immediately() {
        let p = new_fulfilled(42);
        let (st, v) = wait_blocking(&p);
        assert_eq!(st, STATE_FULFILLED);
        assert_eq!(v, 42);
    }

    #[test]
    fn already_rejected_returns_immediately() {
        let p = new_rejected(99);
        let (st, v) = wait_blocking(&p);
        assert_eq!(st, STATE_REJECTED);
        assert_eq!(v, 99);
    }

    #[test]
    fn pending_resolved_in_thread() {
        let p = new_pending();
        let p2 = p.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            resolve(&p2, 7);
        });
        let (st, v) = wait_blocking(&p);
        assert_eq!(st, STATE_FULFILLED);
        assert_eq!(v, 7);
    }

    #[test]
    fn settle_is_idempotent() {
        let p = new_pending();
        assert!(resolve(&p, 1));
        assert!(!resolve(&p, 2));   // segundo resolve e' no-op
        assert!(!reject(&p, 99));    // reject apos resolve tambem no-op
        let (st, v) = wait_blocking(&p);
        assert_eq!(st, STATE_FULFILLED);
        assert_eq!(v, 1);
    }
}
