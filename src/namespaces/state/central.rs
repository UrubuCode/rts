//! Central State Manager - Simplified and optimized for performance
//!
//! Provides centralized state management for namespace-shared state only.
//! Thread-local caches are handled separately for performance.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Simplified state controller for shared namespace state only
pub struct CentralState {
    /// Namespace states that need cross-thread sharing
    namespaces: Mutex<BTreeMap<&'static str, Box<dyn Any + Send + Sync>>>,
}

impl CentralState {
    const fn new() -> Self {
        Self {
            namespaces: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register or get namespace state for cross-thread sharing
    pub fn namespace_state<T: Send + Sync + 'static + Default>(
        &self,
        namespace: &'static str,
    ) -> Arc<Mutex<T>> {
        let mut namespaces = self.lock_or_recover(&self.namespaces);

        if let Some(existing) = namespaces.get(namespace) {
            let any_box = existing.downcast_ref::<Arc<Mutex<T>>>()
                .expect("namespace state type mismatch");
            return any_box.clone();
        }

        let new_state = Arc::new(Mutex::new(T::default()));
        namespaces.insert(namespace, Box::new(new_state.clone()));
        new_state
    }


    fn lock_or_recover<'a, T>(&self, mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// THE single global instance - all state flows through here
static CENTRAL_STATE: OnceLock<CentralState> = OnceLock::new();

/// Get the central state instance
pub fn central() -> &'static CentralState {
    CENTRAL_STATE.get_or_init(CentralState::new)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct TestState {
        value: i32,
    }

    #[test]
    fn namespace_state_works() {
        let state1 = central().namespace_state::<TestState>("test");
        let state2 = central().namespace_state::<TestState>("test");

        // Should be the same instance
        assert_eq!(Arc::as_ptr(&state1), Arc::as_ptr(&state2));

        // Should be able to modify
        {
            let mut guard = state1.lock().unwrap();
            guard.value = 42;
        }

        {
            let guard = state2.lock().unwrap();
            assert_eq!(guard.value, 42);
        }
    }
}