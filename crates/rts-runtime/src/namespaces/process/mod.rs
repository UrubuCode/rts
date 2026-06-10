//! `process` namespace — exit/abort/pid + argv + spawn/wait/kill via
//! std::process.
//!
//! `spawn` recebe os argumentos como uma unica string separada por `\n` (sem
//! arrays na ABF ainda). `wait`/`kill` consomem o handle do filho.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::process::{Child, Command, Stdio};

use rts_engine::abi::ty::{Handle, I32, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry_mut};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Process control: exit/abort, pid, argv, spawn/wait/kill.
#[rts_namespace(process)]
impl ProcessNs {
    /// Termina o processo com `code`.
    #[rts_fn]
    pub fn exit(code: I32) {
        std::process::exit(code)
    }

    /// Aborta o processo (SIGABRT).
    #[rts_fn]
    pub fn abort() {
        std::process::abort()
    }

    /// PID do processo corrente.
    #[rts_fn]
    pub fn pid() -> I64 {
        std::process::id() as i64
    }

    /// Numero de argumentos (alias de env.args_count).
    #[rts_fn]
    pub fn args_count() -> I64 {
        std::env::args().count() as i64
    }

    /// Argumento em `index` como string handle; 0 se fora do range.
    #[rts_fn(ts = "arg_at(index: number): string")]
    pub fn arg_at(index: I64) -> Handle {
        if index < 0 {
            return 0;
        }
        match std::env::args().nth(index as usize) {
            Some(arg) => intern(&arg),
            None => 0,
        }
    }

    /// Dispara um processo filho (args separados por `\n`). Handle opaco, 0 em falha.
    #[rts_fn(ts = "spawn(cmd: string, args_newline_separated: string): number")]
    pub fn spawn(cmd: Str, args: Str) -> Handle {
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
    #[rts_fn]
    pub fn wait(child: U64) -> I32 {
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
    #[rts_fn]
    pub fn kill(child: U64) -> I64 {
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
}
