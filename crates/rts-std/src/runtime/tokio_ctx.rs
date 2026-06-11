//! Context store generico para bridge sync→async.
//!
//! Issue #399 — toda feature async que precisa atravessar o JIT segue
//! o pattern "id u64 opaco + shard map de Arc<T>". Implementado pela
//! primeira vez em `http_server/ops.rs::slots()`; aqui esta abstraido
//! pra reuso em `fetch_async`, `fs.async`, etc.
//!
//! Uso:
//! ```ignore
//! struct MyState { ... }
//! let id = tokio_ctx::put(MyState { ... });
//! // id atravessa o JIT
//! tokio_ctx::with::<MyState, _>(id, |state| { ... });
//! let owned = tokio_ctx::take::<MyState>(id);  // remove
//! ```
//!
//! Cada tipo `T` tem seu proprio shard map (resolvido por `TypeId`)
//! para que ids nao colidam entre features.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

const SHARDS: usize = 32;

type Map = HashMap<u64, Arc<dyn Any + Send + Sync>>;

fn store() -> &'static [Mutex<HashMap<TypeId, Box<[Mutex<Map>; SHARDS]>>>; 1] {
    // Top-level: 1 mutex curto que so' protege a tabela de TypeId →
    // shards. Cada acesso real aos slots lockea o shard correspondente,
    // entao a contention top-level e' rara (so' no primeiro `put` por
    // tipo). Pra simplificar reusamos `[T;1]` como container.
    static S: OnceLock<[Mutex<HashMap<TypeId, Box<[Mutex<Map>; SHARDS]>>>; 1]> = OnceLock::new();
    S.get_or_init(|| std::array::from_fn(|_| Mutex::new(HashMap::new())))
}

fn shards_for<T: 'static>() -> &'static [Mutex<Map>; SHARDS] {
    let tid = TypeId::of::<T>();
    let top = store();
    let mut guard = top[0].lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard
        .entry(tid)
        .or_insert_with(|| Box::new(std::array::from_fn(|_| Mutex::new(HashMap::new()))));
    // SAFETY: o Box vive enquanto o OnceLock — a tabela top so' e'
    // populada (nunca limpa), entao o ponteiro permanece valido pelo
    // resto do processo. Convertemos pra &'static estendendo o lifetime.
    unsafe { &*(entry.as_ref() as *const [Mutex<Map>; SHARDS]) }
}

#[inline]
fn shard_of<T: 'static>(id: u64) -> &'static Mutex<Map> {
    let shards = shards_for::<T>();
    &shards[(id as usize) & (SHARDS - 1)]
}

fn next_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Insere `value` no store e retorna o id u64 que identifica.
pub fn put<T: Send + Sync + 'static>(value: T) -> u64 {
    let id = next_id();
    let arc: Arc<dyn Any + Send + Sync> = Arc::new(value);
    shard_of::<T>(id)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, arc);
    id
}

/// Acessa o valor associado a `id` por referencia. Retorna `None` se o
/// id nao existe ou se o tipo nao bate. Nao remove — chame `take` para
/// consumir.
pub fn with<T: Send + Sync + 'static, R>(id: u64, f: impl FnOnce(&T) -> R) -> Option<R> {
    let map = shard_of::<T>(id).lock().unwrap_or_else(|e| e.into_inner());
    let any = map.get(&id)?.clone();
    drop(map);
    let typed = any.downcast_ref::<T>()?;
    Some(f(typed))
}

/// Remove e retorna o `Arc<T>` associado a `id`. `None` se nao existe
/// ou tipo nao bate.
pub fn take<T: Send + Sync + 'static>(id: u64) -> Option<Arc<T>> {
    let mut map = shard_of::<T>(id).lock().unwrap_or_else(|e| e.into_inner());
    let any = map.remove(&id)?;
    drop(map);
    Arc::downcast::<T>(any).ok()
}

/// Verifica se um id esta vivo no store para o tipo `T`.
pub fn contains<T: Send + Sync + 'static>(id: u64) -> bool {
    let map = shard_of::<T>(id).lock().unwrap_or_else(|e| e.into_inner());
    map.contains_key(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_take_roundtrip() {
        let id = put(String::from("hello"));
        let arc = take::<String>(id).unwrap();
        assert_eq!(*arc, "hello");
        assert!(!contains::<String>(id));
    }

    #[test]
    fn with_does_not_remove() {
        let id = put(42i64);
        let v = with::<i64, _>(id, |n| *n).unwrap();
        assert_eq!(v, 42);
        assert!(contains::<i64>(id));
        let _ = take::<i64>(id);
    }

    #[test]
    fn different_types_do_not_collide() {
        let id_a = put(1u32);
        let id_b = put(String::from("x"));
        // Mesmo que ids batam por sorte, tipos sao isolados.
        assert!(with::<i64, _>(id_a, |_| ()).is_none());
        assert!(with::<i64, _>(id_b, |_| ()).is_none());
        assert_eq!(with::<u32, _>(id_a, |n| *n), Some(1));
        assert_eq!(
            with::<String, _>(id_b, |s| s.clone()),
            Some(String::from("x"))
        );
    }
}
