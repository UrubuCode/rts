//! node:process — the `extern "C"` entry points. Flat, synchronous surface over
//! `std::env` / `std::process` / `std::time`. No hardcoded data: every value is
//! read live from the OS at call time (the only fixed strings are the Node
//! version RTS emulates — the parity target, like `punycode.version`).

use rts_engine::abi::ty::Handle;

use super::words::{array, clock_base, intern, node_arch, node_platform, num_word, object, str_word, throw};

/// The Node major RTS emulates (see docs/node-implementation — "full Node 25 API").
const NODE_VERSION: &str = "v25.0.0";

/// `process.cwd()`.
#[rtse::function(module = "node:process", value = "cwd")]
fn cwd() -> String {
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    cwd.to_string()
}

/// `process.chdir(directory)` — throws on failure.
#[rtse::function(module = "node:process", value = "chdir")]
fn chdir(dir: &str) {
    if let Err(e) = std::env::set_current_dir(dir) {
        throw("Error", &format!("ENOENT: chdir '{dir}': {e}"));
    }
}

/// `process.platform`.
#[rtse::function(module = "node:process", value = "platform")]
fn platform() -> String {
    node_platform().to_string()
}

/// `process.arch`.
#[rtse::function(module = "node:process", value = "arch")]
fn arch() -> String {
    node_arch().to_string()
}

/// `process.pid`.
#[rtse::function(module = "node:process", value = "pid")]
fn pid() -> i64 {
    std::process::id() as i64
}

/// `process.exit([code])`.
#[rtse::function(module = "node:process", value = "exit")]
fn exit(code: i64) {
    std::process::exit(code as i32);
}

/// `process.abort()`.
#[rtse::function(module = "node:process", value = "abort")]
fn abort() {
    std::process::abort();
}

/// `process.uptime()` — seconds since first process-clock access.
#[rtse::function(module = "node:process", value = "uptime")]
fn uptime() -> f64 {
    clock_base().elapsed().as_secs_f64()
}

/// `process.hrtime()` — `[seconds, nanoseconds]` since the clock base.
#[rtse::function(module = "node:process", value = "hrtime")]
fn hrtime() -> Handle {
    let d = clock_base().elapsed();
    array(vec![num_word(d.as_secs() as f64), num_word(d.subsec_nanos() as f64)])
}

/// `process.hrtime(prev)` — the `[sec, nsec]` elapsed since `prev` (a prior
/// `hrtime()` result), matching Node's diff form.
#[rtse::function(module = "node:process", value = "hrtime", overload = "prev")]
fn hrtime_diff(prev: Handle) -> Handle {
    use rts_engine::heap::handles::{with_entry, Entry};
    let (ps, pn) = with_entry(prev, |e| match e {
        Some(Entry::Vec(v)) if v.len() >= 2 => {
            (f64::from_bits(v[0] as u64), f64::from_bits(v[1] as u64))
        }
        _ => (0.0, 0.0),
    });
    let now = clock_base().elapsed();
    let mut sec = now.as_secs() as f64 - ps;
    let mut nsec = now.subsec_nanos() as f64 - pn;
    if nsec < 0.0 {
        nsec += 1e9;
        sec -= 1.0;
    }
    array(vec![num_word(sec), num_word(nsec)])
}

/// `process.version` (`"vX.Y.Z"`).
#[rtse::function(module = "node:process", value = "version")]
fn version() -> String {
    NODE_VERSION.to_string()
}

/// `process.versions` — `{ node, rts }` (RTS is not the V8/uv stack, so it
/// reports only what it genuinely is).
#[rtse::function(module = "node:process", value = "versions")]
fn versions() -> Handle {
    object(
        &["node", "rts"],
        &[str_word(NODE_VERSION.trim_start_matches('v')), str_word(env!("CARGO_PKG_VERSION"))],
    )
}

/// `process.argv` — `[execPath, scriptPath, ...args]` (live from the OS).
#[rtse::function(module = "node:process", value = "argv")]
fn argv() -> Handle {
    let words: Vec<i64> = std::env::args().map(|a| str_word(&a)).collect();
    array(words)
}

/// `process.argv0`.
#[rtse::function(module = "node:process", value = "argv0")]
fn argv0() -> String {
    std::env::args().next().unwrap_or_default().to_string()
}

/// `process.execPath`.
#[rtse::function(module = "node:process", value = "execPath")]
fn exec_path() -> String {
    let p = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    p.to_string()
}

/// `process.title`.
#[rtse::function(module = "node:process", value = "title")]
fn title() -> String {
    let title = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "node".to_string());
    title.to_string()
}

/// `process.getActiveResourcesInfo()` — no async resources tracked yet → `[]`.
#[rtse::function(module = "node:process", value = "getActiveResourcesInfo")]
fn active_resources() -> Handle {
    array(vec![])
}

/// `process.env` — a snapshot object of the current environment. (Reads are
/// live at call time; write-through — `process.env.X = v` reaching `setenv` —
/// is deferred, needs a write-proxy on the object.)
#[rtse::function(module = "node:process", value = "env")]
fn env() -> Handle {
    let vars: Vec<(String, String)> = std::env::vars().collect();
    let keys: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    let values: Vec<i64> = vars.iter().map(|(_, v)| str_word(v)).collect();
    object(&keys, &values)
}
