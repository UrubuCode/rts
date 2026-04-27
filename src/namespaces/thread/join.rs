//! thread::join / thread::detach — consome handle.

use std::thread::JoinHandle;

use super::super::gc::handles::{Entry, free_handle, shard_for_handle};

fn take_join_handle(handle: u64) -> Option<Box<JoinHandle<u64>>> {
    let taken: Option<Box<JoinHandle<u64>>> = {
        let mut guard = shard_for_handle(handle).lock().unwrap();
        match guard.get_mut(handle) {
            Some(entry @ Entry::JoinHandle(_)) => {
                let prev = std::mem::replace(entry, Entry::Free);
                if let Entry::JoinHandle(h) = prev {
                    Some(h)
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    // bump generation via free (no-op if already Free)
    free_handle(handle);
    taken
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_JOIN(handle: u64) -> u64 {
    let Some(jh) = take_join_handle(handle) else {
        return 0;
    };
    match jh.join() {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_DETACH(handle: u64) {
    drop(take_join_handle(handle));
}
