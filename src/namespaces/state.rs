use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

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
        let mut guard = lock_or_recover(&self.result);
        *guard = PromiseResult::Fulfilled(value);
        self.ready.notify_all();
    }

    fn reject(&self, reason: String) {
        let mut guard = lock_or_recover(&self.result);
        *guard = PromiseResult::Rejected(reason);
        self.ready.notify_all();
    }

    fn status(&self) -> PromiseStatus {
        let guard = lock_or_recover(&self.result);
        match &*guard {
            PromiseResult::Pending => PromiseStatus::Pending,
            PromiseResult::Fulfilled(_) => PromiseStatus::Fulfilled,
            PromiseResult::Rejected(_) => PromiseStatus::Rejected,
        }
    }

    fn await_result(&self) -> Result<String, String> {
        let mut guard = lock_or_recover(&self.result);
        loop {
            match &*guard {
                PromiseResult::Pending => {
                    guard = match self.ready.wait(guard) {
                        Ok(next) => next,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                }
                PromiseResult::Fulfilled(value) => return Ok(value.clone()),
                PromiseResult::Rejected(reason) => return Err(reason.clone()),
            }
        }
    }
}

struct RuntimeExecutor {
    sender: mpsc::Sender<RuntimeJob>,
}

impl RuntimeExecutor {
    fn new(worker_count: usize) -> Self {
        let workers = worker_count.max(2);
        let (sender, receiver) = mpsc::channel::<RuntimeJob>();
        let shared_receiver = Arc::new(Mutex::new(receiver));

        for index in 0..workers {
            let worker_receiver = Arc::clone(&shared_receiver);
            let _ = thread::Builder::new()
                .name(format!("rts-async-worker-{index}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let receiver_guard = lock_or_recover(&worker_receiver);
                            receiver_guard.recv()
                        };

                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                });
        }

        Self { sender }
    }

    fn spawn<F>(&self, task: F) -> bool
    where
        F: FnOnce() + Send + 'static,
    {
        self.sender.send(Box::new(task)).is_ok()
    }
}

#[derive(Default)]
struct RuntimeState {
    global: BTreeMap<String, String>,
    buffers: BTreeMap<u64, Vec<u8>>,
    next_buffer_id: u64,
    promises: BTreeMap<u64, Arc<PromiseCell>>,
    next_promise_id: u64,
}

struct RuntimeServices {
    state: Mutex<RuntimeState>,
    executor: RuntimeExecutor,
}

static RUNTIME_SERVICES: OnceLock<RuntimeServices> = OnceLock::new();

fn services() -> &'static RuntimeServices {
    RUNTIME_SERVICES.get_or_init(|| RuntimeServices {
        state: Mutex::new(RuntimeState::default()),
        executor: RuntimeExecutor::new(
            thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(2),
        ),
    })
}

fn lock_state() -> std::sync::MutexGuard<'static, RuntimeState> {
    lock_or_recover(&services().state)
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn global_set(key: impl Into<String>, value: impl Into<String>) {
    let mut state = lock_state();
    state.global.insert(key.into(), value.into());
}

pub fn global_get(key: &str) -> Option<String> {
    let state = lock_state();
    state.global.get(key).cloned()
}

pub fn global_has(key: &str) -> bool {
    let state = lock_state();
    state.global.contains_key(key)
}

pub fn global_delete(key: &str) -> bool {
    let mut state = lock_state();
    state.global.remove(key).is_some()
}

pub fn global_keys_csv() -> String {
    let state = lock_state();
    state.global.keys().cloned().collect::<Vec<_>>().join(",")
}

pub fn buffer_alloc(size: usize) -> u64 {
    let mut state = lock_state();
    state.next_buffer_id = state.next_buffer_id.saturating_add(1);
    let id = state.next_buffer_id;
    state.buffers.insert(id, vec![0; size]);
    id
}

pub fn buffer_free(id: u64) -> bool {
    let mut state = lock_state();
    state.buffers.remove(&id).is_some()
}

pub fn buffer_len(id: u64) -> Option<usize> {
    let state = lock_state();
    state.buffers.get(&id).map(Vec::len)
}

pub fn buffer_read_u8(id: u64, offset: usize) -> Option<u8> {
    let state = lock_state();
    state
        .buffers
        .get(&id)
        .and_then(|buffer| buffer.get(offset).copied())
}

pub fn buffer_write_u8(id: u64, offset: usize, value: u8) -> bool {
    let mut state = lock_state();
    let Some(buffer) = state.buffers.get_mut(&id) else {
        return false;
    };

    if offset >= buffer.len() {
        return false;
    }

    buffer[offset] = value;
    true
}

pub fn buffer_fill(id: u64, value: u8) -> bool {
    let mut state = lock_state();
    let Some(buffer) = state.buffers.get_mut(&id) else {
        return false;
    };
    buffer.fill(value);
    true
}

pub fn buffer_write_text(id: u64, offset: usize, text: &str) -> Option<usize> {
    let mut state = lock_state();
    let buffer = state.buffers.get_mut(&id)?;
    let bytes = text.as_bytes();
    let end = offset.saturating_add(bytes.len());
    if end > buffer.len() {
        buffer.resize(end, 0);
    }
    buffer[offset..end].copy_from_slice(bytes);
    Some(bytes.len())
}

pub fn buffer_read_text(id: u64, offset: usize, length: usize) -> Option<String> {
    let state = lock_state();
    let buffer = state.buffers.get(&id)?;

    if offset > buffer.len() {
        return None;
    }

    let end = offset.saturating_add(length).min(buffer.len());
    let slice = &buffer[offset..end];
    Some(String::from_utf8_lossy(slice).to_string())
}

pub fn buffer_copy(
    src_id: u64,
    dst_id: u64,
    src_offset: usize,
    dst_offset: usize,
    length: usize,
) -> Option<usize> {
    let mut state = lock_state();
    let source = state.buffers.get(&src_id)?;
    if src_offset > source.len() {
        return None;
    }

    let source_end = src_offset.saturating_add(length).min(source.len());
    let payload = source[src_offset..source_end].to_vec();

    let target = state.buffers.get_mut(&dst_id)?;
    let target_end = dst_offset.saturating_add(payload.len());
    if target_end > target.len() {
        target.resize(target_end, 0);
    }

    target[dst_offset..target_end].copy_from_slice(&payload);
    Some(payload.len())
}

pub fn hash_sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();

    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{:02x}", byte));
    }
    output
}

pub fn fs_read_to_string(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| {
        format!(
            "std::fs::read_to_string('{}') failed: {}",
            path.replace('\\', "/"),
            error
        )
    })
}

pub fn fs_read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| {
        format!(
            "std::fs::read('{}') failed: {}",
            path.replace('\\', "/"),
            error
        )
    })
}

pub fn fs_write(path: &str, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|error| {
        format!(
            "std::fs::write('{}') failed: {}",
            path.replace('\\', "/"),
            error
        )
    })
}

pub fn promise_resolve(value: impl Into<String>) -> u64 {
    let value = value.into();
    let (id, promise) = create_pending_promise();
    promise.fulfill(value);
    id
}

pub fn promise_reject(reason: impl Into<String>) -> u64 {
    let reason = reason.into();
    let (id, promise) = create_pending_promise();
    promise.reject(reason);
    id
}

pub fn promise_spawn(task: AsyncTask) -> u64 {
    let (id, promise) = create_pending_promise();
    let promise_ref = Arc::clone(&promise);

    let queued = services().executor.spawn(move || match run_task(task) {
        Ok(value) => promise_ref.fulfill(value),
        Err(reason) => promise_ref.reject(reason),
    });

    if !queued {
        promise.reject("async executor unavailable".to_string());
    }

    id
}

pub fn promise_status(id: u64) -> Option<PromiseStatus> {
    get_promise(id).map(|promise| promise.status())
}

pub fn promise_is_settled(id: u64) -> bool {
    matches!(
        promise_status(id),
        Some(PromiseStatus::Fulfilled | PromiseStatus::Rejected)
    )
}

pub fn promise_await(id: u64) -> Option<Result<String, String>> {
    let promise = get_promise(id)?;
    Some(promise.await_result())
}

fn create_pending_promise() -> (u64, Arc<PromiseCell>) {
    let mut state = lock_state();
    state.next_promise_id = state.next_promise_id.saturating_add(1);
    let id = state.next_promise_id;
    let promise = Arc::new(PromiseCell::pending());
    state.promises.insert(id, Arc::clone(&promise));
    (id, promise)
}

fn get_promise(id: u64) -> Option<Arc<PromiseCell>> {
    let state = lock_state();
    state.promises.get(&id).cloned()
}

fn run_task(task: AsyncTask) -> Result<String, String> {
    match task {
        AsyncTask::Sleep { millis, value } => {
            thread::sleep(Duration::from_millis(millis));
            Ok(value)
        }
        AsyncTask::HashSha256 { data } => Ok(hash_sha256(&data)),
        AsyncTask::ReadTextFile { path } => std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "failed to read file '{}': {}",
                path.replace('\\', "/"),
                error
            )
        }),
        AsyncTask::WriteTextFile { path, content } => std::fs::write(&path, content)
            .map(|_| "ok".to_string())
            .map_err(|error| {
                format!(
                    "failed to write file '{}': {}",
                    path.replace('\\', "/"),
                    error
                )
            }),
        AsyncTask::AppendTextFile { path, content } => {
            use std::io::Write;

            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(content.as_bytes()))
                .map(|_| "ok".to_string())
                .map_err(|error| {
                    format!(
                        "failed to append file '{}': {}",
                        path.replace('\\', "/"),
                        error
                    )
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_and_buffer_roundtrip() {
        global_set("answer", "42");
        assert_eq!(global_get("answer").as_deref(), Some("42"));
        assert!(global_has("answer"));
        assert!(global_delete("answer"));

        let buffer = buffer_alloc(4);
        assert_eq!(buffer_len(buffer), Some(4));
        assert!(buffer_write_u8(buffer, 0, 65));
        assert_eq!(buffer_read_u8(buffer, 0), Some(65));
        assert_eq!(buffer_write_text(buffer, 1, "BC"), Some(2));
        assert_eq!(buffer_read_text(buffer, 0, 3).as_deref(), Some("ABC"));
        assert!(buffer_fill(buffer, b'Z'));
        assert_eq!(buffer_read_text(buffer, 0, 4).as_deref(), Some("ZZZZ"));
        assert!(buffer_free(buffer));
    }

    #[test]
    fn promise_executor_multithread_basics() {
        let resolved = promise_resolve("ok");
        assert_eq!(promise_status(resolved), Some(PromiseStatus::Fulfilled));
        assert_eq!(promise_await(resolved), Some(Ok("ok".to_string())));

        let async_hash = promise_spawn(AsyncTask::HashSha256 {
            data: "hello".to_string(),
        });
        let hash = promise_await(async_hash).expect("promise should exist");
        assert!(hash.is_ok());
    }
}
