//! `process` namespace — exit/abort/pid + argv + spawn/wait/kill via
//! std::process.
//!
//! `spawn` recebe os argumentos como uma unica string separada por `\n` (sem
//! arrays na ABF ainda). `wait`/`kill` consomem o handle do filho.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::process::{Child, Command, Stdio};

use rts_engine::abi::ty::{Handle, I32, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry_mut};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Termina o processo com `code`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_EXIT(code: I32) {
    std::process::exit(code)
}

/// Aborta o processo (SIGABRT).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ABORT() {
    std::process::abort()
}

/// PID do processo corrente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_PID() -> I64 {
    std::process::id() as i64
}

/// Numero de argumentos (alias de env.args_count).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ARGS_COUNT() -> I64 {
    std::env::args().count() as i64
}

/// Argumento em `index` como string handle; 0 se fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ARG_AT(index: I64) -> Handle {
    if index < 0 {
        return 0;
    }
    match std::env::args().nth(index as usize) {
        Some(arg) => intern(&arg),
        None => 0,
    }
}

/// Dispara um processo filho (args separados por `\n`). Handle opaco, 0 em falha.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_SPAWN(
    cmd_ptr: *const u8,
    cmd_len: i64,
    args_ptr: *const u8,
    args_len: i64,
) -> Handle {
    let cmd = match unsafe { rts_engine::abi::str_abi::from_abi(cmd_ptr, cmd_len) } {
        Some(s) => s,
        None => return 0,
    };
    let args = match unsafe { rts_engine::abi::str_abi::from_abi(args_ptr, args_len) } {
        Some(s) => s,
        None => return 0,
    };
    let mut command = Command::new(cmd);
    for line in args.split('\n') {
        if !line.is_empty() {
            command.arg(line);
        }
    }
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match command.spawn() {
        Ok(child) => alloc_entry(Entry::ProcessChild(Box::new(child))),
        Err(_) => 0,
    }
}

/// Aguarda o filho terminar e retorna o exit code (-1 em erro). Consome o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_WAIT(child: U64) -> I32 {
    let taken: Option<Box<Child>> = with_entry_mut(child, |entry| match entry {
        Some(e @ Entry::ProcessChild(_)) => {
            let t = std::mem::replace(e, Entry::Free);
            if let Entry::ProcessChild(c) = t {
                Some(c)
            } else {
                None
            }
        }
        _ => None,
    });
    free_handle(child);
    let Some(mut c) = taken else {
        return -1;
    };
    match c.wait() {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1,
    }
}

/// Mata o filho (SIGKILL / TerminateProcess). Consome o handle. 0 ok, -1 erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_KILL(child: U64) -> I64 {
    let taken: Option<Box<Child>> = with_entry_mut(child, |entry| match entry {
        Some(e @ Entry::ProcessChild(_)) => {
            let t = std::mem::replace(e, Entry::Free);
            if let Entry::ProcessChild(c) = t {
                Some(c)
            } else {
                None
            }
        }
        _ => None,
    });
    free_handle(child);
    let Some(mut c) = taken else {
        return -1;
    };
    match c.kill() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Função `process.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `process` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("process")
        .doc("Process control: exit/abort, pid, argv, spawn/wait/kill.")
        .member(func(
            "exit",
            "__RTS_FN_NS_PROCESS_EXIT",
            Sig::new(vec![AbiType::I32], AbiType::Void),
            "exit(code: number): void",
            "Termina o processo com `code`.",
            __RTS_FN_NS_PROCESS_EXIT as *const u8,
        ))
        .member(func(
            "abort",
            "__RTS_FN_NS_PROCESS_ABORT",
            Sig::new(Vec::new(), AbiType::Void),
            "abort(): void",
            "Aborta o processo (SIGABRT).",
            __RTS_FN_NS_PROCESS_ABORT as *const u8,
        ))
        .member(func(
            "pid",
            "__RTS_FN_NS_PROCESS_PID",
            Sig::new(Vec::new(), AbiType::I64),
            "pid(): number",
            "PID do processo corrente.",
            __RTS_FN_NS_PROCESS_PID as *const u8,
        ))
        .member(func(
            "args_count",
            "__RTS_FN_NS_PROCESS_ARGS_COUNT",
            Sig::new(Vec::new(), AbiType::I64),
            "args_count(): number",
            "Numero de argumentos (alias de env.args_count).",
            __RTS_FN_NS_PROCESS_ARGS_COUNT as *const u8,
        ))
        .member(func(
            "arg_at",
            "__RTS_FN_NS_PROCESS_ARG_AT",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "arg_at(index: number): string",
            "Argumento em `index` como string handle; 0 se fora do range.",
            __RTS_FN_NS_PROCESS_ARG_AT as *const u8,
        ))
        .member(func(
            "spawn",
            "__RTS_FN_NS_PROCESS_SPAWN",
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Handle),
            "spawn(cmd: string, args_newline_separated: string): number",
            "Dispara um processo filho (args separados por `\\n`). Handle opaco, 0 em falha.",
            __RTS_FN_NS_PROCESS_SPAWN as *const u8,
        ))
        .member(func(
            "wait",
            "__RTS_FN_NS_PROCESS_WAIT",
            Sig::new(vec![AbiType::U64], AbiType::I32),
            "wait(child: number): number",
            "Aguarda o filho terminar e retorna o exit code (-1 em erro). Consome o handle.",
            __RTS_FN_NS_PROCESS_WAIT as *const u8,
        ))
        .member(func(
            "kill",
            "__RTS_FN_NS_PROCESS_KILL",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "kill(child: number): number",
            "Mata o filho (SIGKILL / TerminateProcess). Consome o handle. 0 ok, -1 erro.",
            __RTS_FN_NS_PROCESS_KILL as *const u8,
        ))
        // node:process — superfície que o Node expõe em `process` mas que o RTS
        // mora em `env`/`os`. Membros aqui REUSAM os externs nativos (mesmos
        // símbolos), só dão o nome node:process. `import { cwd, platform, arch }
        // from "node:process"` resolve por dado (Member::matches_name), sem o
        // motor nomear `env`/`os`.
        .member(func(
            "cwd",
            "__RTS_FN_NS_ENV_CWD",
            Sig::new(Vec::new(), AbiType::Handle),
            "cwd(): string",
            "Diretório de trabalho corrente (node:process, alias de env.cwd).",
            crate::env::__RTS_FN_NS_ENV_CWD as *const u8,
        ))
        .member(func(
            "platform",
            "__RTS_FN_NS_OS_PLATFORM",
            Sig::new(Vec::new(), AbiType::Handle),
            "platform(): string",
            "Plataforma do SO (node:process, alias de os.platform).",
            crate::os::__RTS_FN_NS_OS_PLATFORM as *const u8,
        ))
        .member(func(
            "arch",
            "__RTS_FN_NS_OS_ARCH",
            Sig::new(Vec::new(), AbiType::Handle),
            "arch(): string",
            "Arquitetura da CPU (node:process, alias de os.arch).",
            crate::os::__RTS_FN_NS_OS_ARCH as *const u8,
        ))
        .done();
}
