//! `spawnSync`, `execSync`, `execFileSync` — the synchronous surface, which
//! is what this engine can answer honestly: each runs to completion on the
//! calling thread and returns a value, exactly what a native entry point can
//! do (see [`crate::fs`]'s module doc for the same argument made about
//! `readFileSync` vs `readFile`).
//!
//! # `execSync`/`execFileSync` do not throw
//!
//! Node throws on a non-zero exit, a signal, or `maxBuffer` exceeded. A
//! native entry point in this crate cannot throw across the call boundary —
//! [`rts_core_rwk::entry::modules`] exposes no such operation, the same gap
//! [`crate::fs`]'s module doc states for its own `Sync` members. So both
//! answer `undefined` in every one of those cases instead of throwing,
//! rather than fabricating a `false`-shaped success. `spawnSync` never had
//! this problem: Node's own `spawnSync` already reports failure through its
//! returned object's `status`/`signal`/`error` fields rather than a throw,
//! so it is unchanged here.
//!
//! # Shell injection — read before calling `execSync` with untrusted input
//!
//! `execSync` always runs `command` through a shell — `/bin/sh -c` on
//! POSIX, `%ComSpec%` (typically `cmd.exe`) on Windows — exactly like
//! Node's own `execSync`/`exec`. A caller building `command` by concatenating
//! unsanitized input has a command injection vulnerability, the same way it
//! would calling Node's `child_process.exec`. `execFileSync`/`spawnSync` do
//! NOT shell out by default and are the safe alternative when the pieces
//! (`file`, `args`) can stay separate.

use std::process::Stdio;

use rts_core_rwk::entry;

use super::capture::{self, Capture};
use super::command;
use super::shared::{option_number, option_text, string, string_array, text};

const DEFAULT_MAX_BUFFER: usize = 1024 * 1024;

/// `spawnSync(command, args?, options?)`.
pub(super) extern "C" fn spawn_sync(_e: u64, _this: u64, command_value: u64, args_value: u64, options: u64, _a3: u64) -> u64 {
    let Some(program) = text(command_value) else {
        return entry::undefined_value();
    };
    let args = string_array(args_value);
    let spec = command::spec_of(&program, args, options);
    let capture = run(&spec, options);
    spawn_sync_result(&spec, &capture, options)
}

/// `execFileSync(file, args?, options?)`.
pub(super) extern "C" fn exec_file_sync(_e: u64, _this: u64, file_value: u64, args_value: u64, options: u64, _a3: u64) -> u64 {
    let Some(program) = text(file_value) else {
        return entry::undefined_value();
    };
    let args = string_array(args_value);
    let spec = command::spec_of(&program, args, options);
    let capture = run(&spec, options);
    stdout_or_undefined(&capture, options)
}

/// `execSync(command, options?)` — see the module doc: always shells out.
pub(super) extern "C" fn exec_sync(_e: u64, _this: u64, command_value: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(line) = text(command_value) else {
        return entry::undefined_value();
    };
    let spec = command::shell_spec(&line, options);
    let capture = run(&spec, options);
    stdout_or_undefined(&capture, options)
}

/// The piece [`spawn_sync`]/[`exec_sync`]/[`exec_file_sync`] all do: build
/// the `Command`, read `input`/`timeout`/`maxBuffer`, run it.
fn run(spec: &command::Spec, options: u64) -> Capture {
    let mut cmd = command::build(spec, options);
    // `stdio: 'inherit'` shares the parent's streams instead of capturing —
    // `capture::run` still pipes stdin/stdout/stderr for the default and
    // `'ignore'` cases, so only this one mode is special-cased here.
    if command::stdio_mode(options) == command::StdioMode::Inherit {
        cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let input = super::shared::option_value(options, "input").and_then(input_bytes);
    let timeout_ms = option_number(options, "timeout");
    let max_buffer = option_number(options, "maxBuffer").map(|value| value.max(0.0) as usize).unwrap_or(DEFAULT_MAX_BUFFER);
    capture::run(cmd, input, timeout_ms, max_buffer)
}

/// `input` as bytes — text encoded UTF-8, or a typed-array/`Buffer` window
/// read via [`entry::bytes_of`]. `entry::bytes_of` needs a context in hand,
/// so this runs inside [`entry::with_runtime`] rather than the ambient form.
fn input_bytes(value: u64) -> Option<Vec<u8>> {
    entry::with_runtime(|context| {
        if let Some(text) = super::shared::text(value) {
            return Some(text.into_bytes());
        }
        entry::bytes_of(context, value)
    })
}

/// `stdout` (or `undefined`) — the return value of `execSync`/`execFileSync`.
/// A failed spawn, a non-zero exit, a signal, or `maxBuffer` exceeded all
/// answer `undefined` rather than throwing; see the module doc.
fn stdout_or_undefined(capture: &Capture, options: u64) -> u64 {
    let clean = capture.error.is_none() && capture.signal.is_none() && capture.status == Some(0);
    if !clean {
        return entry::undefined_value();
    }
    entry::with_runtime(|context| encoded(context, &capture.stdout, options))
}

/// Bytes as a `Buffer` (the default) or as text under `options.encoding`.
fn encoded(context: &mut entry::Context, bytes: &[u8], options: u64) -> u64 {
    match option_text(options, "encoding") {
        Some(name) if name != "buffer" => {
            let text = entry::decode_bytes(bytes, &name);
            entry::make_string(context, &text)
        }
        _ => entry::make_buffer(context, bytes),
    }
}

/// The `SpawnSyncReturns`-shaped object `spawnSync` answers.
fn spawn_sync_result(spec: &command::Spec, capture: &Capture, options: u64) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let pid = capture.pid.map(|pid| entry::make_number(f64::from(pid))).unwrap_or_else(|| entry::undefined_in(context));
        entry::put_member(context, object, "pid", pid);

        let stdout = encoded(context, &capture.stdout, options);
        let stderr = encoded(context, &capture.stderr, options);
        entry::put_member(context, object, "stdout", stdout);
        entry::put_member(context, object, "stderr", stderr);

        let extra_null = entry::null_in(context);
        let output = entry::make_array_in(context, vec![extra_null, stdout, stderr]);
        entry::put_member(context, object, "output", output);

        let status =
            capture.status.map(|code| entry::make_number(f64::from(code))).unwrap_or_else(|| entry::null_in(context));
        entry::put_member(context, object, "status", status);

        let signal = capture.signal.map(string).unwrap_or_else(|| entry::null_in(context));
        entry::put_member(context, object, "signal", signal);

        if let Some((message, code)) = &capture.error {
            let error = entry::make_object(context);
            let message_value = entry::make_string(context, message);
            entry::put_member(context, error, "message", message_value);
            let code_value = entry::make_string(context, code);
            entry::put_member(context, error, "code", code_value);
            entry::put_member(context, object, "error", error);
        }

        // `spawnfile`/`spawnargs` — read straight off `spec`, so a caller
        // can see exactly what this module resolved `shell`/`args` to.
        let spawnfile = entry::make_string(context, &spec.program);
        entry::put_member(context, object, "spawnfile", spawnfile);
        let arg_values: Vec<u64> = spec.args.iter().map(|arg| entry::make_string(context, arg)).collect();
        let spawnargs = entry::make_array_in(context, arg_values);
        entry::put_member(context, object, "spawnargs", spawnargs);

        object
    })
}
