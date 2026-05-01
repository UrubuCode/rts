//! Thread pool global pra acelerar `thread.spawn`-com-detach.
//!
//! Sem pool, cada `thread.spawn(fn, arg)` cria uma OS thread nova
//! (~100µs no Windows). Em workloads tipo http server (1 thread por
//! conexao) isso domina o tempo. Com pool:
//!
//! - N workers pre-criados ficam bloqueados num `Condvar` esperando jobs
//! - `submit(fn_ptr, arg)` empurra job pra fila e acorda 1 worker
//! - worker executa job, retorna pra fila esperando o proximo
//!
//! API:
//! - `try_submit(fn, arg)` — devolve `true` se algum worker pegou; `false`
//!   se todos ocupados (caller cai pra `std::thread::spawn` normal).
//!
//! Pool e' inicializado lazy (no primeiro submit). Tamanho default = 8;
//! configuravel via `RTS_THREAD_POOL_SIZE`.

use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;

struct Job {
    fn_ptr: u64,
    arg: u64,
    /// Variant 2: spawn_with_ud. `ud` e' Some(_) nesse caso e a sig real
    /// e' (u64, u64) -> u64.
    ud: Option<u64>,
}

struct Pool {
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
}

fn pool() -> &'static Pool {
    static P: OnceLock<Pool> = OnceLock::new();
    P.get_or_init(|| {
        let p = Pool {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
        };
        let n: usize = std::env::var("RTS_THREAD_POOL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        for _ in 0..n {
            thread::spawn(move || worker_loop());
        }
        p
    })
}

fn worker_loop() -> ! {
    super::super::gc::thread_registry::register_current();
    loop {
        let job = {
            let mut q = pool().queue.lock().unwrap_or_else(|e| e.into_inner());
            while q.is_empty() {
                q = pool().cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            q.pop().expect("queue not empty")
        };
        // SAFETY: caller garante fn_ptr valido. Mesmas premissas que
        // `__RTS_FN_NS_THREAD_SPAWN`.
        unsafe {
            match job.ud {
                None => {
                    let f: extern "C" fn(u64) -> u64 = std::mem::transmute(job.fn_ptr as usize);
                    let _ = f(job.arg);
                }
                Some(ud) => {
                    let f: extern "C" fn(u64, u64) -> u64 =
                        std::mem::transmute(job.fn_ptr as usize);
                    let _ = f(ud, job.arg);
                }
            }
        }
    }
}

/// Tenta submeter um job ao pool. Retorna `true` em sucesso. O pool
/// nunca rejeita por capacidade — sempre aceita (queue ilimitada). O
/// `bool` permite o caller fallback se quiser.
pub fn submit(fn_ptr: u64, arg: u64) -> bool {
    if fn_ptr == 0 {
        return false;
    }
    let p = pool();
    {
        let mut q = p.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.push(Job { fn_ptr, arg, ud: None });
    }
    p.cv.notify_one();
    true
}

pub fn submit_with_ud(fn_ptr: u64, arg: u64, ud: u64) -> bool {
    if fn_ptr == 0 {
        return false;
    }
    let p = pool();
    {
        let mut q = p.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.push(Job { fn_ptr, arg, ud: Some(ud) });
    }
    p.cv.notify_one();
    true
}
