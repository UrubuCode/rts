//! spawn/wait/kill — Child gerenciado via HandleTable.
//!
//! `args` e uma unica string com argumentos separados por `\n` pra
//! manter a ABI simples (sem arrays ainda). Cada linha vira um arg.
//! Use `""` pra nenhum argumento.

use std::process::{Child, Command, Stdio};

use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Dispara um processo filho. Retorna handle opaco, ou 0 em falha.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_SPAWN(
    cmd_ptr: *const u8,
    cmd_len: i64,
    args_ptr: *const u8,
    args_len: i64,
) -> u64 {
    let Some(cmd) = str_from_abi(cmd_ptr, cmd_len) else {
        return 0;
    };
    let args_str = str_from_abi(args_ptr, args_len).unwrap_or("");

    let mut command = Command::new(cmd);
    for line in args_str.split('\n') {
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

/// Aguarda o filho terminar. Retorna o exit code (ou -1 em erro).
/// Consome o handle — apos wait, o slot vira Free.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_WAIT(handle: u64) -> i32 {
    // Take: move o Child pra fora do Entry, substitui por Free.
    let child: Option<Box<Child>> = {
        let mut guard = shard_for_handle(handle).lock().unwrap();
        match guard.get_mut(handle) {
            Some(entry @ Entry::ProcessChild(_)) => {
                let taken = std::mem::replace(entry, Entry::Free);
                if let Entry::ProcessChild(c) = taken {
                    Some(c)
                } else {
                    None
                }
            }
            _ => None,
        }
    };
    // free formal do slot (bump generation)
    free_handle(handle);
    let Some(mut child) = child else {
        return -1;
    };
    match child.wait() {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1,
    }
}

/// Envia SIGKILL (ou TerminateProcess no Windows). Consome o handle —
/// após kill o slot vira Free (mesmo que kill falhe por processo já
/// terminado). Retorna 0 em sucesso, -1 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_KILL(handle: u64) -> i64 {
    let child: Option<Box<Child>> = {
        let mut guard = shard_for_handle(handle).lock().unwrap();
        match guard.get_mut(handle) {
            Some(entry @ Entry::ProcessChild(_)) => {
                let taken = std::mem::replace(entry, Entry::Free);
                if let Entry::ProcessChild(c) = taken { Some(c) } else { None }
            }
            _ => None,
        }
    };
    free_handle(handle);
    let Some(mut child) = child else { return -1; };
    match child.kill() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
