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

// (#207 timer ordering) Macrotask queue p/ setTimeout(fp, 0): em JS, timers
// rodam na thread principal APOS todas as microtasks drenarem (macrotask >
// microtask). spawn_blocking/thread roda em paralelo e fora de ordem. Esta
// fila (thread-local do main) eh drenada pelo pipeline pos-main, intercalando
// com microtasks na ordem JS spec correta.
thread_local! {
    static MACROTASK_QUEUE: std::cell::RefCell<Vec<(u64, Arc<AtomicBool>, u64)>>
        = const { std::cell::RefCell::new(Vec::new()) };
}

/// Enfileira um setTimeout(fp, 0) como macrotask na thread do main.
fn enqueue_macrotask(fp: u64, flag: Arc<AtomicBool>, handle: u64) {
    MACROTASK_QUEUE.with(|q| q.borrow_mut().push((fp, flag, handle)));
}

/// (#207) Drena a fila de macrotask (setTimeout delay-0) APOS as microtasks.
/// Cada macrotask: se nao cancelada, invoca o callback; depois as microtasks
/// que ela gerou sao drenadas (JS spec: cada macrotask esvazia a microtask
/// queue). Processa em ordem FIFO de registro.
pub fn drain_macrotasks() {
    loop {
        let batch: Vec<(u64, Arc<AtomicBool>, u64)> =
            MACROTASK_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if batch.is_empty() {
            break;
        }
        for (fp, flag, handle) in batch {
            if !flag.load(Ordering::Relaxed) {
                invoke_timer_cb(fp);
            }
            free_handle(handle);
            cancel_timer(handle);
            // JS spec: apos cada macrotask, drena todas as microtasks geradas.
            crate::namespaces::globals::text_encoding::instance::drain_microtasks();
        }
    }
}

fn cancel_timer(handle: u64) {
    if let Some(flag) = timers().lock().unwrap().remove(&handle) {
        flag.store(true, Ordering::Relaxed);
    }
}

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

/// Invoca callback de timer respeitando se `fp` eh fn_ptr raw ou Function
/// handle (caso de resolver de `Promise.withResolvers` ou `new Promise`).
/// INVOKE_AUTO detecta automaticamente.
fn invoke_timer_cb(fp: u64) {
    if fp == 0 {
        return;
    }
    // Se for Function handle valido, usa INVOKE_AUTO; senao transmute direto.
    let is_function_handle = crate::namespaces::gc::handles::with_entry(fp, |e| {
        matches!(e, Some(crate::namespaces::gc::handles::Entry::Function(_)))
    });
    if is_function_handle {
        unsafe extern "C" {
            fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        }
        let empty_args = crate::namespaces::gc::handles::alloc_entry(
            crate::namespaces::gc::handles::Entry::Vec(Box::new(Vec::new())),
        );
        unsafe {
            __RTS_FN_RT_INVOKE_AUTO(fp as i64, 0, empty_args);
        }
    } else {
        unsafe {
            (std::mem::transmute::<u64, CallbackFn>(fp))(0);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TIMERS_SET_TIMEOUT(fp: u64, delay_ms: i64) -> u64 {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let delay = if delay_ms > 0 { delay_ms as u64 } else { 0 };

    let handle = alloc_entry(Entry::Env(vec![0]));

    register_timer(handle, cancelled);

    // (#207 timer ordering) delay 0: macrotask na thread do main (roda APOS
    // microtasks drenarem — ordem JS spec). delay > 0: thread com sleep real.
    if delay == 0 {
        enqueue_macrotask(fp, flag, handle);
        return handle;
    }

    let flag2 = flag.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay));
        if !flag2.load(Ordering::Relaxed) {
            invoke_timer_cb(fp);
        }
        free_handle(handle);
        cancel_timer(handle);
    });
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
            invoke_timer_cb(fp);
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
    // (#207 timer ordering) NAO spawna thread — setImmediate enfileira e roda
    // na "check phase" via drain_immediates (apos microtasks drenarem). O
    // thread::spawn antigo corria em paralelo e podia rodar ANTES das
    // microtasks (ordem errada: immediate|micro em vez de micro|immediate).
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
                invoke_timer_cb(fp);
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
