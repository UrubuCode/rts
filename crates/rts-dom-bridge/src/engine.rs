//! Ponte mínima para invocar callbacks JavaScript armazenados como words ABI.
//!
//! O DOM continua sem conhecer o runtime: este módulo vive no bridge, que já é
//! responsável por atravessar a fronteira `rts-core` ↔ `rts-dom`.

use rts_core::entry::{self, Provided};

use crate::value::integer;

pub const MEMBERS: &[(&str, Provided)] = &[("invoke_cb", invoke_callback)];

/// `engine.invoke_cb(callbackWord, argument)` — chama um callback armazenado pelo
/// DOM com um argumento. O callback cruza como número para a fachada TypeScript,
/// por isso é reconstituído a partir do payload inteiro antes de `entry::call`.
extern "C" fn invoke_callback(
    _e: u64,
    _this: u64,
    callback: u64,
    argument: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let callback = integer(callback, 0) as u64;
    let undefined = entry::undefined_value();
    entry::call(
        callback, undefined, argument, undefined, undefined, undefined,
    )
}
