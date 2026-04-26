//! thread::join / thread::detach — consome handle.

use std::thread::JoinHandle;

use super::super::gc::handles::{Entry, table};

/// Move o JoinHandle pra fora do slot (substituindo por Free) e libera o
/// slot formalmente. Retorna `None` se o handle for invalido ou ja
/// consumido.
fn take_join_handle(handle: u64) -> Option<Box<JoinHandle<u64>>> {
    let taken: Option<Box<JoinHandle<u64>>> = {
        let t = table();
        let mut guard = t.lock().unwrap();
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
    // bump generation no slot (se ja era Free, free retorna false sem efeito)
    table().lock().unwrap().free(handle);
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
    // Drop sem .join() — a thread continua rodando ate completar.
    drop(take_join_handle(handle));
}
