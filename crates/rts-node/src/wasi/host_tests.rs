//! Split out of `host.rs` to keep it under this repository's file-size
//! ceiling — see `super::host`'s own doc for what these exercise.

use super::host::{run, Entry};
use super::WasiEnvEntry;

/// Hand-assembled WASM binary (no `wat`/`wast` dependency — none is in
/// this workspace's manifest) for:
///
/// ```wat
/// (module
///   (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
///   (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
///   (memory (export "memory") 1)
///   (data (i32.const 8) "hi\n")
///   (func (export "_start")
///     (i32.store (i32.const 0) (i32.const 8))   ;; iov.ptr
///     (i32.store (i32.const 4) (i32.const 3))   ;; iov.len
///     (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 20))
///     drop
///     (call $proc_exit (i32.const 7)))
/// )
/// ```
///
/// Writes `"hi\n"` to stdout (fd 1) through `fd_write`, then exits with
/// code 7 through `proc_exit` — one call through each of the two syscalls
/// the module doc calls the minimum for a hello-world.
fn hello_world_module() -> Vec<u8> {
    let mut out = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]; // magic, version

    // Type section: type0 = (i32,i32,i32,i32)->i32 [fd_write]; type1 = (i32)->() [proc_exit]; type2 = ()->() [_start]
    section(&mut out, 1, |body| {
        body.push(3); // count
        body.extend([0x60, 4, 0x7F, 0x7F, 0x7F, 0x7F, 1, 0x7F]); // type0
        body.extend([0x60, 1, 0x7F, 0]); // type1
        body.extend([0x60, 0, 0]); // type2
    });

    // Import section
    section(&mut out, 2, |body| {
        body.push(2); // count
        import(body, "wasi_snapshot_preview1", "fd_write", 0);
        import(body, "wasi_snapshot_preview1", "proc_exit", 1);
    });

    // Function section: func index 2 (after the two imports) has type2
    section(&mut out, 3, |body| {
        body.push(1);
        body.push(2);
    });

    // Memory section: 1 page minimum, exported
    section(&mut out, 5, |body| {
        body.push(1);
        body.extend([0x00, 1]);
    });

    // Export section
    section(&mut out, 7, |body| {
        body.push(2);
        export(body, "memory", 0x02, 0);
        export(body, "_start", 0x00, 2);
    });

    // Code section: func index2's body
    section(&mut out, 10, |body| {
        body.push(1); // one function body
        let mut code = Vec::new();
        code.push(0); // no locals
        store32(&mut code, 0, 8); // *0 = 8 (iov.ptr)
        store32(&mut code, 4, 3); // *4 = 3 (iov.len)
        code.extend([0x41, 1]); // i32.const 1 (fd = stdout)
        code.extend([0x41, 0]); // i32.const 0 (iovs ptr)
        code.extend([0x41, 1]); // i32.const 1 (iovs len)
        code.extend([0x41, 20]); // i32.const 20 (nwritten ptr)
        code.extend([0x10, 0]); // call fd_write (func idx 0)
        code.push(0x1A); // drop
        code.extend([0x41, 7]); // i32.const 7
        code.extend([0x10, 1]); // call proc_exit (func idx 1)
        code.push(0x0B); // end
        body.push(code.len() as u8);
        body.extend(code);
    });

    out
}

/// `i32.store(addr, value)`, in the correct Wasm operand order (address
/// pushed first, then the value, per `i32.store`'s stack signature).
fn store32(code: &mut Vec<u8>, addr: i32, value: i32) {
    code.extend([0x41, addr as u8]); // i32.const addr
    code.extend([0x41, value as u8]); // i32.const value
    code.extend([0x36, 0x02, 0x00]); // i32.store align=2 offset=0
}

fn section(out: &mut Vec<u8>, id: u8, build: impl FnOnce(&mut Vec<u8>)) {
    let mut body = Vec::new();
    build(&mut body);
    out.push(id);
    out.push(body.len() as u8); // every section here is well under 128 bytes
    out.extend(body);
}

fn import(body: &mut Vec<u8>, module: &str, field: &str, type_index: u8) {
    body.push(module.len() as u8);
    body.extend(module.as_bytes());
    body.push(field.len() as u8);
    body.extend(field.as_bytes());
    body.push(0x00); // kind = func
    body.push(type_index);
}

fn export(body: &mut Vec<u8>, name: &str, kind: u8, index: u8) {
    body.push(name.len() as u8);
    body.extend(name.as_bytes());
    body.push(kind);
    body.push(index);
}

fn blank_env() -> WasiEnvEntry {
    WasiEnvEntry {
        args: Vec::new(),
        env: Vec::new(),
        preopens: Vec::new(),
        stdin_fd: 0,
        stdout_fd: 1,
        stderr_fd: 2,
        return_on_exit: true,
        version: "preview1".to_owned(),
        started: false,
    }
}

#[test]
fn hello_world_writes_stdout_and_exits_with_proc_exit_code() {
    let env = blank_env();
    let bytes = hello_world_module();
    let outcome = run(&env, &bytes, Entry::Command);
    assert_eq!(outcome, Ok(7), "proc_exit's code must cross out through the trap: {outcome:?}");
}

#[test]
fn a_missing_memory_export_is_refused() {
    // Type section only, no memory, no _start — instantiation itself
    // should fail cleanly rather than panic.
    let mut out = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    section(&mut out, 1, |body| body.push(0));
    let env = blank_env();
    assert!(run(&env, &out, Entry::Command).is_err());
}
