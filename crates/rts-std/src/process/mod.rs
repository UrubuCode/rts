//! `process` namespace — exit/abort/pid + argv + spawn/wait/kill via
//! std::process.
//!
//! `spawn` recebe os argumentos como uma unica string separada por `\n` (sem
//! arrays na ABF ainda). `wait`/`kill` consomem o handle do filho.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::process::{Child, Command, Stdio};

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry_mut};
use rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Termina o processo com `code`.
#[rtse::function(module = "process", value = "exit")]
fn exit(code: i32) {
    std::process::exit(code)
}

/// Aborta o processo (SIGABRT).
#[rtse::function(module = "process", value = "abort")]
fn abort() {
    std::process::abort()
}

/// PID do processo corrente.
#[rtse::function(module = "process", value = "pid")]
fn pid() -> i64 {
    std::process::id() as i64
}

/// Numero de argumentos (alias de env.args_count).
#[rtse::function(module = "process", value = "args_count")]
fn args_count() -> i64 {
    std::env::args().count() as i64
}

/// Argumento em `index` como string handle; 0 se fora do range.
#[rtse::function(module = "process", value = "arg_at", ret_ts = "string")]
fn arg_at(index: i64) -> Handle {
    if index < 0 {
        return 0;
    }
    match std::env::args().nth(index as usize) {
        Some(arg) => intern(&arg),
        None => 0,
    }
}

/// Dispara um processo filho (args separados por `\n`). Handle opaco, 0 em falha.
#[rtse::function(module = "process", value = "spawn", ret_ts = "number")]
fn spawn(cmd: &str, args: &str) -> Handle {
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
#[rtse::function(module = "process", value = "wait")]
fn wait(child: u64) -> i32 {
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
#[rtse::function(module = "process", value = "kill")]
fn kill(child: u64) -> i64 {
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

/// A generic member with an explicit signature — the escape hatch for the 3
/// `node:process` aliases below, which reuse `env`/`os`'s EXISTING extern
/// symbols verbatim (not a fresh `process.*` body): `#[rtse::function]` always
/// mints a symbol from `module`+`value`, which would give each of these its
/// OWN new symbol instead of resolving through `env.cwd`/`os.platform`/
/// `os.arch`'s single implementation.
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
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `process` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.module("process", |m| {
        m.doc("Process control: exit/abort, pid, argv, spawn/wait/kill.");
        m.registry(exit_entry());
        m.registry(abort_entry());
        m.registry(pid_entry());
        m.registry(args_count_entry());
        m.registry(arg_at_entry());
        m.registry(spawn_entry());
        m.registry(wait_entry());
        m.registry(kill_entry());
        // node:process — superfície que o Node expõe em `process` mas que o RTS
        // mora em `env`/`os`. Membros aqui REUSAM os externs nativos (mesmos
        // símbolos), só dão o nome node:process. `import { cwd, platform, arch }
        // from "node:process"` resolve por dado (Member::matches_name), sem o
        // motor nomear `env`/`os`.
        m.member(func(
            "cwd",
            "__rtsm_env_cwd",
            Sig::new(Vec::new(), AbiType::Handle),
            "cwd(): string",
            "Diretório de trabalho corrente (node:process, alias de env.cwd).",
            crate::env::__rtsm_env_cwd as *const u8,
        ));
        m.member(func(
            "platform",
            "__rtsm_os_platform",
            Sig::new(Vec::new(), AbiType::Handle),
            "platform(): string",
            "Plataforma do SO (node:process, alias de os.platform).",
            crate::os::__rtsm_os_platform as *const u8,
        ));
        m.member(func(
            "arch",
            "__rtsm_os_arch",
            Sig::new(Vec::new(), AbiType::Handle),
            "arch(): string",
            "Arquitetura da CPU (node:process, alias de os.arch).",
            crate::os::__rtsm_os_arch as *const u8,
        ));
    });
}
