//! node:process — the `extern "C"` entry points. Flat, synchronous surface over
//! `std::env` / `std::process` / `std::time`. No hardcoded data: every value is
//! read live from the OS at call time (the only fixed strings are the Node
//! version RTS emulates — the parity target, like `punycode.version`).

use super::words::{array, clock_base, intern, node_arch, node_platform, num_word, object, str_word, throw};

/// The Node major RTS emulates (see docs/node-implementation — "full Node 25 API").
const NODE_VERSION: &str = "v25.0.0";

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `process.cwd()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_CWD() -> u64 {
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    intern(&cwd)
}

/// `process.chdir(directory)` — throws on failure.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_CHDIR(p: *const u8, l: i64) {
    let dir = read(p, l);
    if let Err(e) = std::env::set_current_dir(&dir) {
        throw("Error", &format!("ENOENT: chdir '{dir}': {e}"));
    }
}

/// `process.platform`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_PLATFORM() -> u64 {
    intern(node_platform())
}

/// `process.arch`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ARCH() -> u64 {
    intern(node_arch())
}

/// `process.pid`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_PID() -> i64 {
    std::process::id() as i64
}

/// `process.exit([code])`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_EXIT(code: i64) {
    std::process::exit(code as i32);
}

/// `process.abort()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ABORT() {
    std::process::abort();
}

/// `process.uptime()` — seconds since first process-clock access.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_UPTIME() -> f64 {
    clock_base().elapsed().as_secs_f64()
}

/// `process.hrtime()` — `[seconds, nanoseconds]` since the clock base.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_HRTIME() -> u64 {
    let d = clock_base().elapsed();
    array(vec![num_word(d.as_secs() as f64), num_word(d.subsec_nanos() as f64)])
}

/// `process.hrtime(prev)` — the `[sec, nsec]` elapsed since `prev` (a prior
/// `hrtime()` result), matching Node's diff form.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_HRTIME_DIFF(prev: u64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_VERSION() -> u64 {
    intern(NODE_VERSION)
}

/// `process.versions` — `{ node, rts }` (RTS is not the V8/uv stack, so it
/// reports only what it genuinely is).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_VERSIONS() -> u64 {
    object(
        &["node", "rts"],
        &[str_word(NODE_VERSION.trim_start_matches('v')), str_word(env!("CARGO_PKG_VERSION"))],
    )
}

/// `process.argv` — `[execPath, scriptPath, ...args]` (live from the OS).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ARGV() -> u64 {
    let words: Vec<i64> = std::env::args().map(|a| str_word(&a)).collect();
    array(words)
}

/// `process.argv0`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ARGV0() -> u64 {
    intern(&std::env::args().next().unwrap_or_default())
}

/// `process.execPath`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_EXEC_PATH() -> u64 {
    let p = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    intern(&p)
}

/// `process.title`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_TITLE() -> u64 {
    let title = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "node".to_string());
    intern(&title)
}

/// `process.getActiveResourcesInfo()` — no async resources tracked yet → `[]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ACTIVE_RESOURCES() -> u64 {
    array(vec![])
}

/// `process.env` — a snapshot object of the current environment. (Reads are
/// live at call time; write-through — `process.env.X = v` reaching `setenv` —
/// is deferred, needs a write-proxy on the object.)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_ENV() -> u64 {
    let vars: Vec<(String, String)> = std::env::vars().collect();
    let keys: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    let values: Vec<i64> = vars.iter().map(|(_, v)| str_word(v)).collect();
    object(&keys, &values)
}
