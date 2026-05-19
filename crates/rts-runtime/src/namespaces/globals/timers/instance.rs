use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

use crate::namespaces::gc::handles::{alloc_entry, free_handle, Entry};

use std::collections::HashMap;
use std::sync::Mutex;

static TIMERS: std::sync::OnceLock<Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>> =
    std::sync::OnceLock::new();

fn timers() -> Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>> {
    TIMERS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn register_timer(handle: u64, flag: Arc<AtomicBool>) {
    timers().lock().unwrap().insert(handle, flag);
}

fn cancel_timer(handle: u64) {
    if let Some(flag) = timers().lock().unwrap().remove(&handle) {
        flag.store(true, Ordering::Relaxed);
    }
}

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_SET_TIMEOUT(fp: u64, delay_ms: i64) -> u64 {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let delay = if delay_ms > 0 { delay_ms as u64 } else { 0 };

    let handle = alloc_entry(Entry::Env(vec![0]));

    let flag2 = flag.clone();
    thread::spawn(move || {
        if delay > 0 {
            thread::sleep(Duration::from_millis(delay));
        }
        if !flag2.load(Ordering::Relaxed) && fp != 0 {
            unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
        }
        free_handle(handle);
        cancel_timer(handle);
    });

    register_timer(handle, cancelled);
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_CLEAR_TIMEOUT(handle: u64) {
    cancel_timer(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_SET_INTERVAL(fp: u64, interval_ms: i64) -> u64 {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let ms = if interval_ms > 0 { interval_ms as u64 } else { 1 };

    let handle = alloc_entry(Entry::Env(vec![0]));

    let flag2 = flag.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(ms));
            if flag2.load(Ordering::Relaxed) || fp == 0 {
                break;
            }
            unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
        }
        free_handle(handle);
        cancel_timer(handle);
    });

    register_timer(handle, cancelled);
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_CLEAR_INTERVAL(handle: u64) {
    cancel_timer(handle);
}

/// (cross-runtime #286) setImmediate queue + thread paralela.
/// - Thread spawn (igual setTimeout(0)) garante que `time.sleep_ms` em
///   testes sync veja o callback executar (set_timeout_interval.test.ts).
/// - Queue paralela permite drain_immediates() executar callbacks pos-main
///   ANTES de setTimeout(0) regulares (ordem JS spec).
/// - `ran` AtomicBool previne dupla execucao quando ambos os caminhos
///   chegam ao callback.
type ImmediateEntry = (u64, Arc<AtomicBool>, Arc<AtomicBool>); // (fp, cancelled, ran)
static IMMEDIATE_QUEUE: std::sync::OnceLock<Arc<Mutex<Vec<ImmediateEntry>>>> =
    std::sync::OnceLock::new();

fn immediate_queue() -> Arc<Mutex<Vec<ImmediateEntry>>> {
    IMMEDIATE_QUEUE
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_SET_IMMEDIATE(fp: u64) -> u64 {
    let cancelled = Arc::new(AtomicBool::new(false));
    let ran = Arc::new(AtomicBool::new(false));
    let handle = alloc_entry(Entry::Env(vec![0]));
    immediate_queue().lock().unwrap().push((fp, cancelled.clone(), ran.clone()));

    let flag = cancelled.clone();
    let ran_t = ran.clone();
    thread::spawn(move || {
        if !flag.load(Ordering::Relaxed) && fp != 0
            && !ran_t.swap(true, Ordering::AcqRel)
        {
            unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
        }
        free_handle(handle);
        cancel_timer(handle);
    });
    register_timer(handle, cancelled);
    handle
}

/// Drena setImmediate callbacks pendentes.
pub fn drain_immediates() {
    loop {
        let batch: Vec<ImmediateEntry> = {
            let arc = immediate_queue();
            let mut q = arc.lock().unwrap();
            std::mem::take(&mut *q)
        };
        if batch.is_empty() {
            break;
        }
        for (fp, cancelled, ran) in batch {
            if !cancelled.load(Ordering::Relaxed) && fp != 0
                && !ran.swap(true, Ordering::AcqRel)
            {
                unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
            }
        }
    }
}

/// Drain — bloqueia ate todos timers (setTimeout) ainda nao disparados
/// terminarem. Usado pelo pipeline pos-main para que callbacks de
/// setTimeout possam executar antes do processo sair. Intervals nao
/// sao aguardados (rodariam pra sempre); apenas timers de uma unica
/// execucao (`setTimeout`/`setImmediate`). Por simplicidade aguarda
/// timers ate um deadline maximo de ~5s.
pub fn drain_pending_timers() {
    use std::time::Instant;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pending = timers().lock().unwrap().len();
        if pending == 0 || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE(handle: u64) {
    cancel_timer(handle);
}
