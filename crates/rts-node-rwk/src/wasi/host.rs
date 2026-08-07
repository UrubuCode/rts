//! The WASI preview1 host functions, wired straight into a `wasmi::Linker`
//! and run to completion inside [`run`] — see the parent module doc for why
//! this is where the REAL syscalls live, rather than behind `wasiImport`.
//!
//! # What is linked, and why linking (not just answering `ENOSYS`) is the
//! honest failure for the rest
//!
//! `wasmi::Linker::instantiate` fails outright if the module imports a name
//! this file never defines — which is the correct behaviour for a syscall
//! this module has no implementation for AT ALL (`poll_oneoff`, every
//! `sock_*`, `path_link`, `path_symlink`, `path_readlink`,
//! `path_filestat_set_times`, `fd_renumber`, `fd_datasync`, `fd_pread`,
//! `fd_pwrite`, `fd_allocate`, `fd_advise`, `fd_fdstat_set_flags`,
//! `fd_fdstat_set_rights`, `fd_filestat_set_size`, `fd_filestat_set_times`):
//! a module needing one of those cannot run here, and fails at `start`
//! rather than mid-execution on the specific call. The functions that ARE
//! linked but always answer failure (`fd_prestat_get`, `fd_seek`,
//! `fd_readdir`, `path_open`, `path_create_directory`,
//! `path_remove_directory`, `path_unlink_file`, `path_rename`,
//! `path_filestat_get`) are the ones a typical `wasi-libc`-built binary's
//! own startup probes even when it does not need them — most concretely
//! `fd_prestat_get`, which libc calls with an increasing fd starting at 3
//! until it answers `EBADF`, to discover which fds are preopened
//! directories. Refusing to link it at all would break that probe's own
//! loop rather than the filesystem access it is checking for; answering
//! `EBADF`/`ENOSYS` unconditionally is what tells it "no preopens" without
//! ever touching a real path.

use super::errno;
use super::WasiEnvEntry;
use std::fmt;
use wasmi::{Caller, Engine, Linker, Module, Store};

/// Whether a run should start via `_start` (a WASI COMMAND) or `_initialize`
/// (a WASI REACTOR) — see the reference doc §2.1's either/or rule.
#[derive(Clone, Copy)]
pub(super) enum Entry {
    Command,
    Reactor,
}

/// State the host functions close over — a snapshot of the env the `WASI`
/// constructor fixed, plus nothing that changes over the run except through
/// `__wasi_proc_exit`, which is carried out-of-band as a trap (see
/// [`ProcExit`]) rather than as a mutable field here.
struct HostState {
    args: Vec<String>,
    /// Pre-formatted `"KEY=VALUE\0"` lines — the exact wire shape
    /// `environ_get` copies into linear memory, built once rather than on
    /// every call.
    env_lines: Vec<String>,
    stdin_fd: i32,
    stdout_fd: i32,
    stderr_fd: i32,
}

/// Carries `__wasi_proc_exit`'s code out through a trap — `wasmi` has no
/// other way for a host function with no return value to stop execution
/// early, and `proc_exit` never returns to its caller by spec.
#[derive(Debug)]
struct ProcExit(i32);

impl fmt::Display for ProcExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "__wasi_proc_exit({})", self.0)
    }
}

impl wasmi::core::HostError for ProcExit {}

/// Compiles `wasm_bytes`, links the preview1 host functions in
/// `wasi_snapshot_preview1`, instantiates, and runs the module per
/// `entry_kind`. `Ok(code)` is the process exit code (`0` unless
/// `proc_exit` was called); `Err` covers every failure this function
/// cannot recover from — bad bytes, a missing `memory` export, the wrong
/// entry point exported for `entry_kind`, or a trap that was not
/// `proc_exit`.
pub(super) fn run(env: &WasiEnvEntry, wasm_bytes: &[u8], entry_kind: Entry) -> Result<i32, String> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes).map_err(|error| error.to_string())?;

    let env_lines = env.env.iter().map(|(key, value)| format!("{key}={value}\0")).collect();
    let state = HostState {
        args: env.args.clone(),
        env_lines,
        stdin_fd: env.stdin_fd,
        stdout_fd: env.stdout_fd,
        stderr_fd: env.stderr_fd,
    };
    let mut store = Store::new(&engine, state);
    let linker = build_linker(&engine).map_err(|error| error.to_string())?;

    let instance_pre = linker.instantiate(&mut store, &module).map_err(|error| error.to_string())?;
    let instance = instance_pre.start(&mut store).map_err(|error| error.to_string())?;

    if instance.get_export(&store, "memory").and_then(|export| export.into_memory()).is_none() {
        return Err("module exports no `memory`".to_owned());
    }

    let export_name = match entry_kind {
        Entry::Command => "_start",
        Entry::Reactor => "_initialize",
    };
    let Some(entry_func) = instance.get_export(&store, export_name).and_then(|export| export.into_func()) else {
        return Err(format!("module exports no `{export_name}`"));
    };
    let typed = entry_func
        .typed::<(), ()>(&store)
        .map_err(|error| error.to_string())?;

    match typed.call(&mut store, ()) {
        Ok(()) => Ok(0),
        Err(trap) => match trap.downcast_ref::<ProcExit>() {
            Some(ProcExit(code)) => Ok(*code),
            None => Err(trap.to_string()),
        },
    }
}

fn build_linker(engine: &Engine) -> Result<Linker<HostState>, wasmi::errors::LinkerError> {
    let mut linker = Linker::new(engine);
    let ns = "wasi_snapshot_preview1";

    linker.func_wrap(ns, "args_sizes_get", args_sizes_get)?;
    linker.func_wrap(ns, "args_get", args_get)?;
    linker.func_wrap(ns, "environ_sizes_get", environ_sizes_get)?;
    linker.func_wrap(ns, "environ_get", environ_get)?;
    linker.func_wrap(ns, "clock_res_get", clock_res_get)?;
    linker.func_wrap(ns, "clock_time_get", clock_time_get)?;
    linker.func_wrap(ns, "random_get", random_get)?;
    linker.func_wrap(ns, "proc_exit", proc_exit)?;
    linker.func_wrap(ns, "sched_yield", sched_yield)?;
    linker.func_wrap(ns, "fd_write", fd_write)?;
    linker.func_wrap(ns, "fd_read", fd_read)?;
    linker.func_wrap(ns, "fd_close", fd_close)?;
    linker.func_wrap(ns, "fd_fdstat_get", fd_fdstat_get)?;
    linker.func_wrap(ns, "fd_prestat_get", fd_prestat_get)?;
    linker.func_wrap(ns, "fd_prestat_dir_name", fd_prestat_dir_name)?;
    linker.func_wrap(ns, "fd_seek", fd_seek)?;
    linker.func_wrap(ns, "fd_readdir", fd_readdir)?;
    linker.func_wrap(ns, "path_open", path_open)?;
    linker.func_wrap(ns, "path_filestat_get", path_filestat_get)?;
    linker.func_wrap(ns, "path_create_directory", path_create_directory)?;
    linker.func_wrap(ns, "path_remove_directory", path_remove_directory)?;
    linker.func_wrap(ns, "path_unlink_file", path_unlink_file)?;
    linker.func_wrap(ns, "path_rename", path_rename)?;
    Ok(linker)
}

// --------------------------------------------------------------- helpers --

fn memory_of(caller: &Caller<'_, HostState>) -> Option<wasmi::Memory> {
    caller.get_export("memory").and_then(wasmi::Extern::into_memory)
}

fn write_u32(memory: &wasmi::Memory, caller: &mut Caller<'_, HostState>, at: i32, value: u32) -> i32 {
    match memory.write(&mut *caller, at as usize, &value.to_le_bytes()) {
        Ok(()) => errno::SUCCESS,
        Err(_) => errno::EINVAL,
    }
}

fn write_u64(memory: &wasmi::Memory, caller: &mut Caller<'_, HostState>, at: i32, value: u64) -> i32 {
    match memory.write(&mut *caller, at as usize, &value.to_le_bytes()) {
        Ok(()) => errno::SUCCESS,
        Err(_) => errno::EINVAL,
    }
}

// ------------------------------------------------------------- args/env --

fn args_sizes_get(mut caller: Caller<'_, HostState>, argc_ptr: i32, argv_buf_size_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let count = caller.data().args.len() as u32;
    let size: u32 = caller.data().args.iter().map(|arg| arg.len() as u32 + 1).sum();
    let a = write_u32(&memory, &mut caller, argc_ptr, count);
    let b = write_u32(&memory, &mut caller, argv_buf_size_ptr, size);
    if a != errno::SUCCESS { a } else { b }
}

fn args_get(mut caller: Caller<'_, HostState>, argv_ptr: i32, argv_buf_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let args = caller.data().args.clone();
    write_pointer_table(&memory, &mut caller, argv_ptr, argv_buf_ptr, &args)
}

fn environ_sizes_get(mut caller: Caller<'_, HostState>, environc_ptr: i32, environ_buf_size_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let count = caller.data().env_lines.len() as u32;
    let size: u32 = caller.data().env_lines.iter().map(|line| line.len() as u32).sum();
    let a = write_u32(&memory, &mut caller, environc_ptr, count);
    let b = write_u32(&memory, &mut caller, environ_buf_size_ptr, size);
    if a != errno::SUCCESS { a } else { b }
}

fn environ_get(mut caller: Caller<'_, HostState>, environ_ptr: i32, environ_buf_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    // Each line already carries its own trailing NUL (see `HostState::env_lines`),
    // so the shared writer below (which appends one per entry) is still correct —
    // it just adds a second NUL, which `environ_get`'s own consumers accept
    // as an ordinary empty tail byte.
    let lines: Vec<String> = caller.data().env_lines.iter().map(|line| line.trim_end_matches('\0').to_owned()).collect();
    write_pointer_table(&memory, &mut caller, environ_ptr, environ_buf_ptr, &lines)
}

/// Shared by `args_get`/`environ_get`: writes each string's NUL-terminated
/// bytes into `buf_ptr`, and a `u32` pointer to each into `table_ptr`.
fn write_pointer_table(memory: &wasmi::Memory, caller: &mut Caller<'_, HostState>, table_ptr: i32, buf_ptr: i32, items: &[String]) -> i32 {
    let mut cursor = buf_ptr as u32;
    let mut pointers = Vec::with_capacity(items.len());
    for item in items {
        pointers.push(cursor);
        let mut bytes = item.as_bytes().to_vec();
        bytes.push(0);
        if memory.write(&mut *caller, cursor as usize, &bytes).is_err() {
            return errno::EINVAL;
        }
        cursor += bytes.len() as u32;
    }
    for (index, pointer) in pointers.iter().enumerate() {
        let at = table_ptr + (index as i32) * 4;
        let result = write_u32(memory, caller, at, *pointer);
        if result != errno::SUCCESS {
            return result;
        }
    }
    errno::SUCCESS
}

// ------------------------------------------------------------------ time --

fn clock_res_get(mut caller: Caller<'_, HostState>, _clock_id: i32, resolution_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    write_u64(&memory, &mut caller, resolution_ptr, 1)
}

fn clock_time_get(mut caller: Caller<'_, HostState>, _clock_id: i32, _precision: i64, time_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    write_u64(&memory, &mut caller, time_ptr, nanos)
}

fn random_get(mut caller: Caller<'_, HostState>, buf_ptr: i32, buf_len: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let mut bytes = vec![0u8; buf_len.max(0) as usize];
    if getrandom::fill(&mut bytes).is_err() {
        return errno::EIO;
    }
    match memory.write(&mut caller, buf_ptr as usize, &bytes) {
        Ok(()) => errno::SUCCESS,
        Err(_) => errno::EINVAL,
    }
}

// -------------------------------------------------------------- control --

fn proc_exit(_caller: Caller<'_, HostState>, code: i32) -> Result<(), wasmi::core::Trap> {
    Err(wasmi::core::Trap::from(ProcExit(code)))
}

fn sched_yield(_caller: Caller<'_, HostState>) -> i32 {
    std::thread::yield_now();
    errno::SUCCESS
}

// ------------------------------------------------------------------- fds --

/// One `(ptr, len)` iovec, as `fd_write`/`fd_read` receive them.
fn read_iovecs(memory: &wasmi::Memory, caller: &Caller<'_, HostState>, iovs_ptr: i32, iovs_len: i32) -> Option<Vec<(u32, u32)>> {
    let mut out = Vec::with_capacity(iovs_len.max(0) as usize);
    for index in 0..iovs_len {
        let mut raw = [0u8; 8];
        memory.read(caller, (iovs_ptr + index * 8) as usize, &mut raw).ok()?;
        let ptr = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let len = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        out.push((ptr, len));
    }
    Some(out)
}

fn fd_write(mut caller: Caller<'_, HostState>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    let (stdout_fd, stderr_fd) = (caller.data().stdout_fd, caller.data().stderr_fd);
    let target: Option<fn(&[u8])> = if fd == stdout_fd {
        Some(|bytes| {
            use std::io::Write;
            let _ = std::io::stdout().write_all(bytes);
        })
    } else if fd == stderr_fd {
        Some(|bytes| {
            use std::io::Write;
            let _ = std::io::stderr().write_all(bytes);
        })
    } else {
        None
    };
    let Some(write_to) = target else {
        return errno::EBADF;
    };
    let Some(iovecs) = read_iovecs(&memory, &caller, iovs_ptr, iovs_len) else {
        return errno::EINVAL;
    };
    let mut total = 0u32;
    for (ptr, len) in iovecs {
        let mut buf = vec![0u8; len as usize];
        if memory.read(&caller, ptr as usize, &mut buf).is_err() {
            return errno::EINVAL;
        }
        write_to(&buf);
        total += len;
    }
    write_u32(&memory, &mut caller, nwritten_ptr, total)
}

fn fd_read(mut caller: Caller<'_, HostState>, fd: i32, iovs_ptr: i32, iovs_len: i32, nread_ptr: i32) -> i32 {
    let Some(memory) = memory_of(&caller) else {
        return errno::EINVAL;
    };
    if fd != caller.data().stdin_fd {
        return errno::EBADF;
    }
    let Some(iovecs) = read_iovecs(&memory, &caller, iovs_ptr, iovs_len) else {
        return errno::EINVAL;
    };
    use std::io::Read;
    let mut total = 0u32;
    for (ptr, len) in iovecs {
        let mut buf = vec![0u8; len as usize];
        let read = std::io::stdin().read(&mut buf).unwrap_or(0);
        if memory.write(&mut caller, ptr as usize, &buf[..read]).is_err() {
            return errno::EINVAL;
        }
        total += read as u32;
        if read < len as usize {
            break; // short read: stdin exhausted for this call, same as a real fd.
        }
    }
    write_u32(&memory, &mut caller, nread_ptr, total)
}

fn fd_close(caller: Caller<'_, HostState>, fd: i32) -> i32 {
    let state = caller.data();
    match fd == state.stdin_fd || fd == state.stdout_fd || fd == state.stderr_fd {
        true => errno::SUCCESS,
        false => errno::EBADF,
    }
}

fn fd_fdstat_get(caller: Caller<'_, HostState>, fd: i32, _stat_ptr: i32) -> i32 {
    // A real answer needs to fill the 24-byte `fdstat` struct at
    // `_stat_ptr`; not attempted here (no fd-type/rights model exists in
    // this module beyond the three standard streams), so a caller reading
    // the struct after a `SUCCESS` answer would read zeroed/stale bytes.
    // Answering `EBADF` for anything but the three standard streams is
    // still correct and is what matters for the libc preopen-probe loop
    // `fd_prestat_get` documents.
    let state = caller.data();
    match fd == state.stdin_fd || fd == state.stdout_fd || fd == state.stderr_fd {
        true => errno::SUCCESS,
        false => errno::EBADF,
    }
}

/// Always `EBADF` — see the module doc: this is what tells a `wasi-libc`
/// program's own preopen-discovery loop "there are none" without it having
/// to understand why, rather than failing to link at all.
fn fd_prestat_get(_caller: Caller<'_, HostState>, _fd: i32, _prestat_ptr: i32) -> i32 {
    errno::EBADF
}

fn fd_prestat_dir_name(_caller: Caller<'_, HostState>, _fd: i32, _path_ptr: i32, _path_len: i32) -> i32 {
    errno::EBADF
}

fn fd_seek(_caller: Caller<'_, HostState>, _fd: i32, _offset: i64, _whence: i32, _newoffset_ptr: i32) -> i32 {
    errno::ENOSYS
}

fn fd_readdir(_caller: Caller<'_, HostState>, _fd: i32, _buf: i32, _buf_len: i32, _cookie: i64, _bufused_ptr: i32) -> i32 {
    errno::ENOSYS
}

// -------------------------------------------------------- path_* (refused) --
//
// No `preopens` mapping is consulted anywhere in this module (see the
// parent module doc) — every one of these answers failure unconditionally,
// regardless of what a `WASI` instance's `preopens` option held.

fn path_open(
    _caller: Caller<'_, HostState>,
    _fd: i32,
    _dirflags: i32,
    _path_ptr: i32,
    _path_len: i32,
    _oflags: i32,
    _fs_rights_base: i64,
    _fs_rights_inheriting: i64,
    _fdflags: i32,
    _opened_fd_ptr: i32,
) -> i32 {
    errno::ENOTCAPABLE
}

fn path_filestat_get(_caller: Caller<'_, HostState>, _fd: i32, _flags: i32, _path_ptr: i32, _path_len: i32, _stat_ptr: i32) -> i32 {
    errno::ENOTCAPABLE
}

fn path_create_directory(_caller: Caller<'_, HostState>, _fd: i32, _path_ptr: i32, _path_len: i32) -> i32 {
    errno::ENOTCAPABLE
}

fn path_remove_directory(_caller: Caller<'_, HostState>, _fd: i32, _path_ptr: i32, _path_len: i32) -> i32 {
    errno::ENOTCAPABLE
}

fn path_unlink_file(_caller: Caller<'_, HostState>, _fd: i32, _path_ptr: i32, _path_len: i32) -> i32 {
    errno::ENOTCAPABLE
}

fn path_rename(_caller: Caller<'_, HostState>, _fd: i32, _old_path_ptr: i32, _old_path_len: i32, _new_fd: i32, _new_path_ptr: i32, _new_path_len: i32) -> i32 {
    errno::ENOTCAPABLE
}

