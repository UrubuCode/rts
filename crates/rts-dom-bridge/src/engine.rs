//! Ponte mínima para invocar callbacks JavaScript armazenados como words ABI.
//!
//! O DOM continua sem conhecer o runtime: este módulo vive no bridge, que já é
//! responsável por atravessar a fronteira `rts-core` ↔ `rts-dom`.

use rts_core::entry::{self, Provided};

use crate::value::{integer, nothing};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("invoke_cb", invoke_callback),
    ("run_event_loop", run_event_loop),
    ("take_error", take_error),
];

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

/// `engine.run_event_loop()` — fecha o task da página.
///
/// Drena o que os `<script>` enfileiraram. Sem isto, um `.then` ou um
/// `queueMicrotask` registado por um script ficava na fila para sempre: o
/// callback nunca acontecia e nada dizia porquê.
extern "C" fn run_event_loop(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::drain_microtasks();
    nothing()
}

/// `engine.take_error()` — o erro que uma microtask deixou pendente, e limpa-o.
///
/// `undefined` quando não houve nenhum.
///
/// Existe porque um throw dentro de uma microtask não passa por nenhum
/// `try`/`catch` de `.ts`: viaja num canal lateral do motor, e quem quiser
/// isolar a página como o console de um browser faz — reportar e seguir — tem
/// de o consumir explicitamente. Não consumir é pior do que parece: o slot
/// continua marcado, e a próxima verificação lê o erro de outra pessoa como se
/// fosse seu.
extern "C" fn take_error(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if entry::thrown() == 0 {
        return nothing();
    }
    entry::take_thrown()
}
