//! node:child_process — the SYNCHRONOUS (blocking) surface over `std::process`:
//! `execSync` (shell command → stdout, throws on non-zero exit), `spawnSync`
//! (program + args → a result object), `execFileSync` (program + args → stdout).
//! Real child processes — no stubs. (Async `spawn`/`exec` + the `ChildProcess`
//! stream class need the event-loop/stream subsystems and are deferred.)
//!
//! Node returns Buffers for captured output; RTS returns UTF-8 strings (the
//! common `{ encoding: 'utf8' }` form) — the bytes are real, the type differs.

use std::process::{Command, Stdio};

use rts_engine::heap::handles::{with_entry, Entry};
use rts_engine::heap::poly::poly_handle_normalize;
use rts_engine::heap::shapes::{alloc_shaped_object, null_word, string_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn throw(message: &str) {
    unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, message.as_ptr(), message.len() as i64) };
}

fn num(v: f64) -> i64 {
    v.to_bits() as i64
}

/// Read a JS `string[]` argument into a `Vec<String>`.
fn read_str_array(handle: u64) -> Vec<String> {
    with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|&w| {
                poly_handle_normalize(w as u64)
                    .map(|h| {
                        with_entry(h, |e2| match e2 {
                            Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
                            _ => String::new(),
                        })
                    })
                    .unwrap_or_default()
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// A shell `Command` for the platform (`cmd /C` / `/bin/sh -c`).
fn shell(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(command);
        c
    }
}

/// `execSync(command)` → stdout string; throws on non-zero exit (carrying the
/// stderr text, like Node's `ExecException`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CP_EXEC(p: *const u8, l: i64) -> u64 {
    let command = read(p, l);
    match shell(&command).output() {
        Ok(out) => {
            if out.status.success() {
                intern(&String::from_utf8_lossy(&out.stdout))
            } else {
                let code = out.status.code().unwrap_or(-1);
                let msg = format!("Command failed: {command} (exit {code})\n{}", String::from_utf8_lossy(&out.stderr).trim_end());
                throw(msg.trim_end());
                intern("")
            }
        }
        Err(e) => {
            throw(&format!("Command failed: {command}: {e}"));
            intern("")
        }
    }
}

/// `execFileSync(file, args)` → stdout string; throws on non-zero exit.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CP_EXEC_FILE(p: *const u8, l: i64, args: u64) -> u64 {
    let file = read(p, l);
    match Command::new(&file).args(read_str_array(args)).output() {
        Ok(out) => {
            if out.status.success() {
                intern(&String::from_utf8_lossy(&out.stdout))
            } else {
                throw(&format!("Command failed: {file} (exit {})", out.status.code().unwrap_or(-1)));
                intern("")
            }
        }
        Err(e) => {
            throw(&format!("spawnSync {file}: {e}"));
            intern("")
        }
    }
}

/// `spawnSync(command, args)` → `{ pid, status, signal, stdout, stderr }`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CP_SPAWN(p: *const u8, l: i64, args: u64) -> u64 {
    let command = read(p, l);
    let spawn = Command::new(&command)
        .args(read_str_array(args))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    match spawn {
        Ok(child) => {
            let pid = child.id();
            match child.wait_with_output() {
                Ok(out) => alloc_shaped_object(
                    &["pid", "status", "signal", "stdout", "stderr"],
                    &[
                        num(pid as f64),
                        num(out.status.code().unwrap_or(-1) as f64),
                        null_word() as i64,
                        string_word(&out.stdout) as i64,
                        string_word(&out.stderr) as i64,
                    ],
                ),
                Err(e) => spawn_error(&command, &e.to_string()),
            }
        }
        Err(e) => spawn_error(&command, &e.to_string()),
    }
}

/// A `spawnSync` result for a spawn/wait failure (`status: null`, an `error`
/// field), matching Node's error-object shape.
fn spawn_error(command: &str, message: &str) -> u64 {
    alloc_shaped_object(
        &["pid", "status", "signal", "stdout", "stderr", "error"],
        &[
            num(0.0),
            null_word() as i64,
            null_word() as i64,
            string_word(b"") as i64,
            string_word(b"") as i64,
            string_word(format!("spawnSync {command}: {message}").as_bytes()) as i64,
        ],
    )
}
