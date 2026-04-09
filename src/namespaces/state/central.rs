//! Central State Manager - THE single point of control for ALL runtime state
//!
//! This is the ONLY place where state can be stored in the RTS runtime.
//! Every namespace, cache, buffer, handle, etc. MUST go through this controller.
//!
//! This enables the GC to track and manage ALL allocations from a single point.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

/// The single, global state controller for the entire RTS runtime
pub struct CentralState {
    /// All namespaces states indexed by name
    namespaces: Mutex<BTreeMap<&'static str, Box<dyn Any + Send + Sync>>>,

    /// All caches indexed by cache identifier
    caches: Mutex<BTreeMap<String, Box<dyn Any + Send + Sync>>>,

    /// All runtime handles indexed by handle type and ID
    handles: Mutex<BTreeMap<(TypeId, u64), Box<dyn Any + Send + Sync>>>,

    /// Next available handle ID for each type
    next_handle_ids: Mutex<BTreeMap<TypeId, u64>>,

    /// Allocation tracking for GC
    allocations: Mutex<BTreeMap<usize, AllocationInfo>>,
}

#[derive(Debug, Clone)]
pub struct AllocationInfo {
    pub type_name: &'static str,
    pub size: usize,
    pub timestamp: std::time::Instant,
    pub namespace: Option<&'static str>,
}

impl CentralState {
    const fn new() -> Self {
        Self {
            namespaces: Mutex::new(BTreeMap::new()),
            caches: Mutex::new(BTreeMap::new()),
            handles: Mutex::new(BTreeMap::new()),
            next_handle_ids: Mutex::new(BTreeMap::new()),
            allocations: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register or get namespace state
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

        // Track allocation
        self.track_allocation::<T>(Some(namespace));

        let new_state = Arc::new(Mutex::new(T::default()));
        namespaces.insert(namespace, Box::new(new_state.clone()));
        new_state
    }

    /// Register or get cache
    pub fn cache<T: Send + Sync + 'static + Default>(
        &self,
        cache_id: &str,
    ) -> Arc<Mutex<T>> {
        let mut caches = self.lock_or_recover(&self.caches);

        if let Some(existing) = caches.get(cache_id) {
            let any_box = existing.downcast_ref::<Arc<Mutex<T>>>()
                .expect("cache type mismatch");
            return any_box.clone();
        }

        // Track allocation
        self.track_allocation::<T>(None);

        let new_cache = Arc::new(Mutex::new(T::default()));
        caches.insert(cache_id.to_string(), Box::new(new_cache.clone()));
        new_cache
    }

    /// Create a new handle of type T
    pub fn create_handle<T: Send + Sync + 'static>(&self, value: T) -> u64 {
        let type_id = TypeId::of::<T>();

        // Get next handle ID for this type
        let mut next_ids = self.lock_or_recover(&self.next_handle_ids);
        let handle_id = next_ids.entry(type_id).or_insert(0);
        *handle_id += 1;
        let id = *handle_id;
        drop(next_ids);

        // Store the handle
        let mut handles = self.lock_or_recover(&self.handles);
        handles.insert((type_id, id), Box::new(value));

        // Track allocation
        self.track_allocation::<T>(None);

        id
    }

    /// Get handle value by ID - returns cloned value to avoid lifetime issues
    pub fn get_handle<T: Send + Sync + 'static + Clone>(&self, handle_id: u64) -> Option<T> {
        let type_id = TypeId::of::<T>();
        let handles = self.lock_or_recover(&self.handles);

        handles.get(&(type_id, handle_id))
            .and_then(|boxed| boxed.downcast_ref::<T>())
            .cloned()
    }

    /// Execute closure with mutable access to handle value
    pub fn with_handle_mut<T: Send + Sync + 'static, R>(
        &self,
        handle_id: u64,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        let type_id = TypeId::of::<T>();
        let mut handles = self.lock_or_recover(&self.handles);

        handles.get_mut(&(type_id, handle_id))
            .and_then(|boxed| boxed.downcast_mut::<T>())
            .map(f)
    }

    /// Remove handle
    pub fn remove_handle<T: Send + Sync + 'static>(&self, handle_id: u64) -> bool {
        let type_id = TypeId::of::<T>();
        let mut handles = self.lock_or_recover(&self.handles);
        handles.remove(&(type_id, handle_id)).is_some()
    }

    /// Get allocation statistics for GC
    pub fn allocation_stats(&self) -> BTreeMap<&'static str, (usize, usize)> {
        let allocations = self.lock_or_recover(&self.allocations);
        let mut stats: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();

        for info in allocations.values() {
            let (count, total_size) = stats.entry(info.type_name).or_insert((0, 0));
            *count += 1;
            *total_size += info.size;
        }

        stats
    }

    /// Clear old allocations (called by GC)
    pub fn gc_sweep(&self, max_age: std::time::Duration) {
        let mut allocations = self.lock_or_recover(&self.allocations);
        let now = std::time::Instant::now();

        allocations.retain(|_, info| {
            now.duration_since(info.timestamp) <= max_age
        });
    }

    fn track_allocation<T: 'static>(&self, namespace: Option<&'static str>) {
        let mut allocations = self.lock_or_recover(&self.allocations);
        let ptr = &self as *const _ as usize;

        let info = AllocationInfo {
            type_name: std::any::type_name::<T>(),
            size: std::mem::size_of::<T>(),
            timestamp: std::time::Instant::now(),
            namespace,
        };

        allocations.insert(ptr, info);
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

/// Convenience macro for getting namespace state
#[macro_export]
macro_rules! namespace_state {
    ($ns:literal, $type:ty) => {
        $crate::namespaces::state::central::central().namespace_state::<$type>($ns)
    };
}

/// Convenience macro for getting cache
#[macro_export]
macro_rules! cache_state {
    ($cache_id:expr, $type:ty) => {
        $crate::namespaces::state::central::central().cache::<$type>($cache_id)
    };
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

    #[test]
    fn cache_works() {
        let cache1 = central().cache::<TestState>("my-cache");
        let cache2 = central().cache::<TestState>("my-cache");

        // Should be the same instance
        assert_eq!(Arc::as_ptr(&cache1), Arc::as_ptr(&cache2));
    }

    #[test]
    fn handles_work() {
        let id = central().create_handle("test value".to_string());

        let value = central().get_handle::<String>(id);
        assert_eq!(value, Some("test value".to_string()));

        // Test with_handle_mut
        central().with_handle_mut::<String, ()>(id, |value| {
            value.push_str(" modified");
        });

        let modified_value = central().get_handle::<String>(id);
        assert_eq!(modified_value, Some("test value modified".to_string()));

        assert!(central().remove_handle::<String>(id));
        assert_eq!(central().get_handle::<String>(id), None);
    }
}