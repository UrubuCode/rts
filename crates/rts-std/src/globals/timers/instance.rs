use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use rts_engine::heap::handles::{alloc_entry, free_handle, Entry};

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

// (#207 timer ordering / cross-runtime #393) Macrotask queue para TODO
// setTimeout (delay 0 e >0). Em JS os timers rodam na thread principal,
// ordenados por (deadline, ordem-de-registro). Antes: delay-0 ia para esta
// fila (FIFO) e delay>0 spawnava UMA THREAD por timer — threads paralelas
// disparavam fora de ordem (dois setTimeout(x,10) podiam imprimir invertido).
// Agora ambos viram entradas com `deadline` absoluto + `seq` monotonico; o
// pump dispara em ordem (deadline, seq) deterministica. setInterval continua
// em thread (periodico) e setImmediate na fila propria.
struct Macrotask {
    fp: u64,
    flag: Arc<AtomicBool>,
    handle: u64,
    deadline: std::time::Instant,
    seq: u64,
}
thread_local! {
    static MACROTASK_QUEUE: std::cell::RefCell<Vec<Macrotask>>
        = const { std::cell::RefCell::new(Vec::new()) };
}
static MACROTASK_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_macrotask_seq() -> u64 {
    MACROTASK_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Enfileira um setTimeout(fp, delay) como macrotask com deadline absoluto.
fn enqueue_macrotask(fp: u64, flag: Arc<AtomicBool>, handle: u64, delay_ms: u64) {
    let deadline = std::time::Instant::now() + Duration::from_millis(delay_ms);
    let seq = next_macrotask_seq();
    MACROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Macrotask {
            fp,
            flag,
            handle,
            deadline,
            seq,
        })
    });
}

/// Menor deadline entre macrotasks pendentes nao-canceladas (None se vazia).
pub fn next_macrotask_deadline() -> Option<std::time::Instant> {
    MACROTASK_QUEUE.with(|q| {
        q.borrow()
            .iter()
            .filter(|m| !m.flag.load(Ordering::Relaxed))
            .map(|m| m.deadline)
            .min()
    })
}

/// Dispara TODAS as macrotasks ja' vencidas (`deadline <= now`) em ordem
/// (deadline, seq). Cancela/remove entradas ja' canceladas. Apos cada callback
/// drena as microtasks geradas (JS spec: cada macrotask esvazia a microtask
/// queue). Retorna quando nenhuma macrotask esta vencida.
pub fn pump_due_macrotasks() {
    loop {
        let now = std::time::Instant::now();
        let next = MACROTASK_QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            // Descarta canceladas (clearTimeout).
            q.retain(|m| {
                let cancelled = m.flag.load(Ordering::Relaxed);
                if cancelled {
                    free_handle(m.handle);
                }
                !cancelled
            });
            // Acha a vencida com menor (deadline, seq).
            let mut best: Option<usize> = None;
            for (i, m) in q.iter().enumerate() {
                if m.deadline <= now {
                    let better = match best {
                        None => true,
                        Some(b) => (m.deadline, m.seq) < (q[b].deadline, q[b].seq),
                    };
                    if better {
                        best = Some(i);
                    }
                }
            }
            best.map(|i| q.remove(i))
        });
        match next {
            Some(m) => {
                invoke_timer_cb(m.fp);
                free_handle(m.handle);
                cancel_timer(m.handle);
                crate::globals::text_encoding::instance::drain_microtasks();
            }
            None => break,
        }
    }
}

/// (#207) Drena a fila de macrotask (setTimeout) APOS as microtasks. Usada
/// pelo pipeline pos-main: dispara as vencidas e, enquanto restarem timers
/// futuros, dorme ate o proximo deadline (cap de 5s) e repete — preservando
/// ordem (deadline, seq).
pub fn drain_macrotasks() {
    let cap = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        pump_due_macrotasks();
        let Some(next) = next_macrotask_deadline() else {
            break;
        };
        if next > cap {
            break;
        }
        let now = std::time::Instant::now();
        if next > now {
            thread::sleep(next - now);
        }
    }
}

/// (cross-runtime #393) Pump dirigido por tempo para `time.sleep_ms`: dispara
/// microtasks/immediates/macrotasks vencidas e dorme ate o proximo deadline
/// (ou ate `target`), repetindo — assim `setTimeout(cb,10)` dispara DENTRO de
/// `sleep_ms(50)` (set_timeout_interval) mas em ordem deterministica.
pub fn pump_until(target: std::time::Instant) {
    loop {
        crate::globals::text_encoding::instance::drain_microtasks();
        drain_immediates();
        pump_due_macrotasks();
        let now = std::time::Instant::now();
        if now >= target {
            break;
        }
        let wake = next_macrotask_deadline()
            .map(|d| d.min(target))
            .unwrap_or(target);
        let now2 = std::time::Instant::now();
        if wake > now2 {
            thread::sleep(wake - now2);
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
    let is_function_handle = rts_engine::heap::handles::with_entry(fp, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::Function(_)))
    });
    if is_function_handle {
        unsafe extern "C" {
            fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        }
        let empty_args = rts_engine::heap::handles::alloc_entry(
            rts_engine::heap::handles::Entry::Vec(Box::new(Vec::new())),
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

    // (#207 timer ordering / cross-runtime #393) delay 0 e delay>0 entram na
    // MESMA fila ordenada por (deadline, seq), drenada na thread do main. Sem
    // thread-per-timer: dois setTimeout(_, 10) disparam na ordem de registro
    // (deterministico). `time.sleep_ms` faz pump dirigido por tempo, entao um
    // setTimeout(cb, 10) ainda dispara dentro de sleep_ms(50).
    enqueue_macrotask(fp, flag, handle, delay);
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
    let ms = if interval_ms > 0 {
        interval_ms as u64
    } else {
        1
    };

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
    immediate_queue()
        .lock()
        .unwrap()
        .push((fp, cancelled.clone(), ran.clone()));
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
            if !cancelled.load(Ordering::Relaxed) && fp != 0 && !ran.swap(true, Ordering::AcqRel) {
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
