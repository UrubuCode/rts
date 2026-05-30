//! (#310/#311/#312) Override runtime de metodos de console.
//!
//! O codegen despacha `console.log/group/table/...` por pattern sintatico
//! (compile-time), entao reatribuir `(console as any).group = fn` em runtime
//! era ignorado. Aqui mantemos um side-table thread-local
//! `method -> Function handle`. O assignment grava nele; o call site checa
//! antes de cair no builtin nativo.
//!
//! Reatribuir para o valor original (capturado antes via `const g =
//! console.group`) volta o override a 0 (slot apagado) — `console.group`
//! captura devolve o sentinel 0 (sem handle), entao SET_OVERRIDE(method, 0)
//! limpa o slot e o call site volta ao nativo.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// method-name -> (Function handle, variadic). handle 0 = sem override.
    static CONSOLE_OVERRIDES: RefCell<HashMap<String, (i64, bool)>> =
        RefCell::new(HashMap::new());
}

/// `(console as any).<method> = fn` — grava o override. `fn == 0` apaga.
/// `variadic != 0` indica callback `(...args)` — o call site empacota todos
/// os args num unico array (#310).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CONSOLE_SET_OVERRIDE(
    method_ptr: *const u8,
    method_len: i64,
    fn_handle: i64,
    variadic: i64,
) {
    let method = read_method(method_ptr, method_len);
    CONSOLE_OVERRIDES.with(|c| {
        let mut m = c.borrow_mut();
        if fn_handle == 0 {
            m.remove(&method);
        } else {
            m.insert(method, (fn_handle, variadic != 0));
        }
    });
}

/// `console.<method>(...)` call site — devolve o handle do override ou 0
/// (nativo). O codegen, se != 0, invoca via INVOKE_AUTO com os args.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CONSOLE_GET_OVERRIDE(
    method_ptr: *const u8,
    method_len: i64,
) -> i64 {
    let method = read_method(method_ptr, method_len);
    CONSOLE_OVERRIDES.with(|c| c.borrow().get(&method).map(|(h, _)| *h).unwrap_or(0))
}

/// (#310) True (1) se o override do metodo eh variadic (`...args`) — o call
/// site empacota os args num unico array antes de INVOKE_AUTO.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_CONSOLE_OVERRIDE_IS_VARIADIC(
    method_ptr: *const u8,
    method_len: i64,
) -> i64 {
    let method = read_method(method_ptr, method_len);
    CONSOLE_OVERRIDES.with(|c| {
        c.borrow().get(&method).map(|(_, v)| *v as i64).unwrap_or(0)
    })
}

fn read_method(ptr: *const u8, len: i64) -> String {
    if ptr.is_null() || len <= 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(slice).into_owned()
}
