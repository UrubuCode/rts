use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

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

#[derive(Debug)]
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

    pub fn status(&self) -> PromiseStatus {
        match &*self.result.lock().unwrap() {
            PromiseResult::Pending => PromiseStatus::Pending,
            PromiseResult::Fulfilled(_) => PromiseStatus::Fulfilled,
            PromiseResult::Rejected(_) => PromiseStatus::Rejected,
        }
    }

    pub fn await_result(&self) -> Result<String, String> {
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

// ── Executor ─────────────────────────────────────────────────────────────────

type RuntimeJob = Box<dyn FnOnce() + Send + 'static>;

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
                .spawn(move || {
                    loop {
                        match rx.lock().unwrap_or_else(|p| p.into_inner()).recv() {
                            Ok(f) => f(),
                            Err(_) => break,
                        }
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
struct PromiseStore {
    promises: BTreeMap<u64, Arc<PromiseCell>>,
    next_id: u64,
}

static PROMISE_STORE: OnceLock<Arc<Mutex<PromiseStore>>> = OnceLock::new();
static EXECUTOR: OnceLock<RuntimeExecutor> = OnceLock::new();

fn promise_store() -> Arc<Mutex<PromiseStore>> {
    PROMISE_STORE
        .get_or_init(|| Arc::new(Mutex::new(PromiseStore::default())))
        .clone()
}

fn executor() -> &'static RuntimeExecutor {
    EXECUTOR.get_or_init(|| {
        RuntimeExecutor::new(
            thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(2),
        )
    })
}

fn alloc_promise() -> (u64, Arc<PromiseCell>) {
    let store = promise_store();
    let mut s = store.lock().unwrap();
    s.next_id = s.next_id.saturating_add(1);
    let id = s.next_id;
    let cell = Arc::new(PromiseCell::pending());
    s.promises.insert(id, Arc::clone(&cell));
    (id, cell)
}

fn find_promise(id: u64) -> Option<Arc<PromiseCell>> {
    promise_store().lock().unwrap().promises.get(&id).cloned()
}

// ── Public API ────────────────────────────────────────────────────────────────

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
    let queued = executor().spawn(move || match run_task(task) {
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

fn run_task(task: AsyncTask) -> Result<String, String> {
    match task {
        AsyncTask::Sleep { millis, value } => {
            thread::sleep(Duration::from_millis(millis));
            Ok(value)
        }
        AsyncTask::HashSha256 { data } => {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(data.as_bytes());
            let digest = h.finalize();
            let mut out = String::with_capacity(digest.len() * 2);
            for byte in digest {
                out.push_str(&format!("{byte:02x}"));
            }
            Ok(out)
        }
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
