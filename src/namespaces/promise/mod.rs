//! `promise` namespace — primitivas de Promise<T> async (issue #412).
//!
//! Diferente de `Entry::Promise(i64)` (Promise sincrona ja' resolvida —
//! caminho rapido legado de `globals/fetch`), aqui temos
//! `Entry::PromiseAsync(Arc<PromiseSlot>)` com state machine completo:
//! pending/fulfilled/rejected + waiters via tokio oneshot.
//!
//! API ABI:
//! - `promise.new()` → handle pending
//! - `promise.resolve(h, value)` → settle Fulfilled
//! - `promise.reject(h, error)` → settle Rejected
//! - `promise.state(h)` → 0 (pending) / 1 (fulfilled) / 2 (rejected)
//! - `promise.wait(h)` → bloqueia ate settle, retorna value
//!   (rejected → trap pra integrar com try/catch — F5)
//! - `promise.try_value(h)` → nao-bloqueante, 0 se ainda pending
//!
//! `Promise.resolve(v)` / `Promise.reject(e)` (semantica JS) sao
//! atalhos: criar slot ja' settled.

pub mod abi;
pub mod ops;
