//! Runtime state for built-in namespaces.
//!
//! Each subsystem owns a `static OnceLock<Arc<Mutex<T>>>` — there is no
//! central registry. Thread-local caches (parser, expr cache, etc.) live in
//! `thread_local!` blocks inside the namespace that needs them.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

// ── Globals ─────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct GlobalsState {
    pub global: BTreeMap<String, String>,
}

static GLOBALS: OnceLock<Arc<Mutex<GlobalsState>>> = OnceLock::new();

fn globals() -> Arc<Mutex<GlobalsState>> {
    GLOBALS
        .get_or_init(|| Arc::new(Mutex::new(GlobalsState::default())))
        .clone()
}

pub struct Globals;

impl Globals {
    pub fn set(key: impl Into<String>, value: impl Into<String>) {
        globals().lock().unwrap().global.insert(key.into(), value.into());
    }

    pub fn get(key: &str) -> Option<String> {
        globals().lock().unwrap().global.get(key).cloned()
    }

    pub fn has(key: &str) -> bool {
        globals().lock().unwrap().global.contains_key(key)
    }

    pub fn delete(key: &str) -> bool {
        globals().lock().unwrap().global.remove(key).is_some()
    }

    pub fn keys_csv() -> String {
        globals()
            .lock()
            .unwrap()
            .global
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    }
}

// ── Async task types ─────────────────────────────────────────────────────────

type RuntimeJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug, Clone)]
pub enum AsyncTask {
    Sleep { millis: u64, value: String },
    HashSha256 { data: String },
    ReadTextFile { path: String },
    WriteTextFile { path: String, content: String },
    AppendTextFile { path: String, content: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromiseStatus {
    Pending,
    Fulfilled,
    Rejected,
}

impl PromiseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fulfilled => "fulfilled",
            Self::Rejected => "rejected",
        }
    }
}

// ── Promise cell ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum PromiseResult {
    Pending,
    Fulfilled(String),
    Rejected(String),
}

#[derive(Debug)]
struct PromiseCell {
    result: Mutex<PromiseResult>,
    ready: Condvar,
}

impl PromiseCell {
    fn pending() -> Self {
        Self {
            result: Mutex::new(PromiseResult::Pending),
            ready: Condvar::new(),
        }
    }

    fn fulfill(&self, value: String) {
        *self.result.lock().unwrap() = PromiseResult::Fulfilled(value);
        self.ready.notify_all();
    }

    fn reject(&self, reason: String) {
        *self.result.lock().unwrap() = PromiseResult::Rejected(reason);
        self.ready.notify_all();
    }

    fn status(&self) -> PromiseStatus {
        match &*self.result.lock().unwrap() {
            PromiseResult::Pending => PromiseStatus::Pending,
            PromiseResult::Fulfilled(_) => PromiseStatus::Fulfilled,
            PromiseResult::Rejected(_) => PromiseStatus::Rejected,
        }
    }

    fn await_result(&self) -> Result<String, String> {
        let mut guard = self.result.lock().unwrap();
        loop {
            match &*guard {
                PromiseResult::Pending => {
                    guard = self.ready.wait(guard).unwrap_or_else(|p| p.into_inner());
                }
                PromiseResult::Fulfilled(v) => return Ok(v.clone()),
                PromiseResult::Rejected(r) => return Err(r.clone()),
            }
        }
    }
}

// ── Async executor ───────────────────────────────────────────────────────────

struct RuntimeExecutor {
    sender: mpsc::Sender<RuntimeJob>,
}

impl RuntimeExecutor {
    fn new(worker_count: usize) -> Self {
        let workers = worker_count.max(2);
        let (sender, receiver) = mpsc::channel::<RuntimeJob>();
        let shared = Arc::new(Mutex::new(receiver));

        for index in 0..workers {
            let rx = Arc::clone(&shared);
            let _ = thread::Builder::new()
                .name(format!("rts-async-worker-{index}"))
                .spawn(move || loop {
                    let job = rx.lock().unwrap_or_else(|p| p.into_inner()).recv();
                    match job {
                        Ok(f) => f(),
                        Err(_) => break,
                    }
                });
        }

        Self { sender }
    }

    fn spawn(&self, task: impl FnOnce() + Send + 'static) -> bool {
        self.sender.send(Box::new(task)).is_ok()
    }
}

// ── Runtime state ─────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct RuntimeState {
    buffers: BTreeMap<u64, Vec<u8>>,
    next_buffer_id: u64,
    promises: BTreeMap<u64, Arc<PromiseCell>>,
    next_promise_id: u64,
}

static RUNTIME: OnceLock<Arc<Mutex<RuntimeState>>> = OnceLock::new();

fn runtime() -> Arc<Mutex<RuntimeState>> {
    RUNTIME
        .get_or_init(|| Arc::new(Mutex::new(RuntimeState::default())))
        .clone()
}

struct RuntimeServices {
    executor: RuntimeExecutor,
}

static SERVICES: OnceLock<RuntimeServices> = OnceLock::new();

fn services() -> &'static RuntimeServices {
    SERVICES.get_or_init(|| RuntimeServices {
        executor: RuntimeExecutor::new(
            thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(2),
        ),
    })
}

// ── Public API — globals ─────────────────────────────────────────────────────

pub fn global_set(key: impl Into<String>, value: impl Into<String>) {
    Globals::set(key, value);
}

pub fn global_get(key: &str) -> Option<String> {
    Globals::get(key)
}

pub fn global_has(key: &str) -> bool {
    Globals::has(key)
}

pub fn global_delete(key: &str) -> bool {
    Globals::delete(key)
}

pub fn global_keys_csv() -> String {
    Globals::keys_csv()
}

// ── Public API — buffers ─────────────────────────────────────────────────────

pub fn buffer_alloc(size: usize) -> u64 {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    s.next_buffer_id = s.next_buffer_id.saturating_add(1);
    let id = s.next_buffer_id;
    s.buffers.insert(id, vec![0; size]);
    id
}

pub fn buffer_free(id: u64) -> bool {
    let arc = runtime();
    arc.lock().unwrap().buffers.remove(&id).is_some()
}

pub fn buffer_len(id: u64) -> Option<usize> {
    let arc = runtime();
    arc.lock().unwrap().buffers.get(&id).map(Vec::len)
}

pub fn buffer_read_u8(id: u64, offset: usize) -> Option<u8> {
    let arc = runtime();
    let s = arc.lock().unwrap();
    s.buffers.get(&id).and_then(|b| b.get(offset).copied())
}

pub fn buffer_write_u8(id: u64, offset: usize, value: u8) -> bool {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    let Some(buf) = s.buffers.get_mut(&id) else { return false };
    if offset >= buf.len() { return false; }
    buf[offset] = value;
    true
}

pub fn buffer_fill(id: u64, value: u8) -> bool {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    let Some(buf) = s.buffers.get_mut(&id) else { return false };
    buf.fill(value);
    true
}

pub fn buffer_write_text(id: u64, offset: usize, text: &str) -> Option<usize> {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    let buf = s.buffers.get_mut(&id)?;
    let bytes = text.as_bytes();
    let end = offset.saturating_add(bytes.len());
    if end > buf.len() { buf.resize(end, 0); }
    buf[offset..end].copy_from_slice(bytes);
    Some(bytes.len())
}

pub fn buffer_read_text(id: u64, offset: usize, length: usize) -> Option<String> {
    let arc = runtime();
    let s = arc.lock().unwrap();
    let buf = s.buffers.get(&id)?;
    if offset > buf.len() { return None; }
    let end = offset.saturating_add(length).min(buf.len());
    Some(String::from_utf8_lossy(&buf[offset..end]).into_owned())
}

pub fn buffer_copy(
    src_id: u64,
    dst_id: u64,
    src_offset: usize,
    dst_offset: usize,
    length: usize,
) -> Option<usize> {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    let src = s.buffers.get(&src_id)?;
    if src_offset > src.len() { return None; }
    let src_end = src_offset.saturating_add(length).min(src.len());
    let payload = src[src_offset..src_end].to_vec();

    let dst = s.buffers.get_mut(&dst_id)?;
    let dst_end = dst_offset.saturating_add(payload.len());
    if dst_end > dst.len() { dst.resize(dst_end, 0); }
    dst[dst_offset..dst_end].copy_from_slice(&payload);
    Some(payload.len())
}

// ── Public API — crypto ──────────────────────────────────────────────────────

pub fn hash_sha256(value: &str) -> String {
    let mut h = Sha256::new();
    h.update(value.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ── Public API — filesystem ──────────────────────────────────────────────────

pub fn fs_read_to_string(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("std::fs::read_to_string('{}') failed: {e}", path.replace('\\', "/")))
}

pub fn fs_read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path)
        .map_err(|e| format!("std::fs::read('{}') failed: {e}", path.replace('\\', "/")))
}

pub fn fs_write(path: &str, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data)
        .map_err(|e| format!("std::fs::write('{}') failed: {e}", path.replace('\\', "/")))
}

// ── Public API — promises ────────────────────────────────────────────────────

pub fn promise_resolve(value: impl Into<String>) -> u64 {
    let (id, cell) = alloc_promise();
    cell.fulfill(value.into());
    id
}

pub fn promise_reject(reason: impl Into<String>) -> u64 {
    let (id, cell) = alloc_promise();
    cell.reject(reason.into());
    id
}

pub fn promise_spawn(task: AsyncTask) -> u64 {
    let (id, cell) = alloc_promise();
    let cell_ref = Arc::clone(&cell);

    let queued = services().executor.spawn(move || match run_task(task) {
        Ok(v) => cell_ref.fulfill(v),
        Err(e) => cell_ref.reject(e),
    });

    if !queued {
        cell.reject("async executor unavailable".to_string());
    }

    id
}

pub fn promise_status(id: u64) -> Option<PromiseStatus> {
    find_promise(id).map(|c| c.status())
}

pub fn promise_is_settled(id: u64) -> bool {
    matches!(
        promise_status(id),
        Some(PromiseStatus::Fulfilled | PromiseStatus::Rejected)
    )
}

pub fn promise_await(id: u64) -> Option<Result<String, String>> {
    Some(find_promise(id)?.await_result())
}

// ── Internals ────────────────────────────────────────────────────────────────

fn alloc_promise() -> (u64, Arc<PromiseCell>) {
    let arc = runtime();
    let mut s = arc.lock().unwrap();
    s.next_promise_id = s.next_promise_id.saturating_add(1);
    let id = s.next_promise_id;
    let cell = Arc::new(PromiseCell::pending());
    s.promises.insert(id, Arc::clone(&cell));
    (id, cell)
}

fn find_promise(id: u64) -> Option<Arc<PromiseCell>> {
    runtime().lock().unwrap().promises.get(&id).cloned()
}

fn run_task(task: AsyncTask) -> Result<String, String> {
    match task {
        AsyncTask::Sleep { millis, value } => {
            thread::sleep(Duration::from_millis(millis));
            Ok(value)
        }
        AsyncTask::HashSha256 { data } => Ok(hash_sha256(&data)),
        AsyncTask::ReadTextFile { path } => std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read '{}': {e}", path.replace('\\', "/"))),
        AsyncTask::WriteTextFile { path, content } => std::fs::write(&path, content)
            .map(|_| "ok".to_string())
            .map_err(|e| format!("failed to write '{}': {e}", path.replace('\\', "/"))),
        AsyncTask::AppendTextFile { path, content } => {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
                .map(|_| "ok".to_string())
                .map_err(|e| format!("failed to append '{}': {e}", path.replace('\\', "/")))
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_and_buffer_roundtrip() {
        global_set("answer", "42");
        assert_eq!(global_get("answer").as_deref(), Some("42"));
        assert!(global_has("answer"));
        assert!(global_delete("answer"));

        let id = buffer_alloc(4);
        assert_eq!(buffer_len(id), Some(4));
        assert!(buffer_write_u8(id, 0, 65));
        assert_eq!(buffer_read_u8(id, 0), Some(65));
        assert_eq!(buffer_write_text(id, 1, "BC"), Some(2));
        assert_eq!(buffer_read_text(id, 0, 3).as_deref(), Some("ABC"));
        assert!(buffer_fill(id, b'Z'));
        assert_eq!(buffer_read_text(id, 0, 4).as_deref(), Some("ZZZZ"));
        assert!(buffer_free(id));
    }

    #[test]
    fn promise_executor_basics() {
        let resolved = promise_resolve("ok");
        assert_eq!(promise_status(resolved), Some(PromiseStatus::Fulfilled));
        assert_eq!(promise_await(resolved), Some(Ok("ok".to_string())));

        let h = promise_spawn(AsyncTask::HashSha256 { data: "hello".to_string() });
        let result = promise_await(h).expect("promise must exist");
        assert!(result.is_ok());
    }
}
