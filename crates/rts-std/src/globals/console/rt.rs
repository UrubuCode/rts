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
pub extern "C" fn __RTS_FN_RT_CONSOLE_GET_OVERRIDE(method_ptr: *const u8, method_len: i64) -> i64 {
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
    CONSOLE_OVERRIDES.with(|c| c.borrow().get(&method).map(|(_, v)| *v as i64).unwrap_or(0))
}

fn read_method(ptr: *const u8, len: i64) -> String {
    if ptr.is_null() || len <= 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(slice).into_owned()
}

// ── Drain do console p/ runtime (doutrina: console é não-primordial → Registry/
// runtime, zero lógica de método no motor) ───────────────────────────────────
// O codegen mantém só a coerção-display per-arg (genérica, dependente de tipo
// compile-time) e empacota dois Vecs: `display_vec` (handles de string já
// coergidos p/ exibição) e `raw_vec` (valores crus i64, p/ override/assert). Toda
// a lógica POR MÉTODO (qual fd, join, override-dispatch, assert) vive aqui.

fn vec_len(h: u64) -> i64 {
    rts_shared::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h)
}
fn vec_get(h: u64, i: i64) -> i64 {
    rts_shared::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i)
}
fn str_new(s: &[u8]) -> u64 {
    crate::collector::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
}
fn str_concat(a: u64, b: u64) -> u64 {
    crate::collector::string_pool::__RTS_FN_NS_GC_STRING_CONCAT(a, b)
}

/// Junta os handles de `vec[start..]` com " " entre eles (semântica JS). Começa
/// de `seed` (ex.: o prefixo "Assertion failed:" do assert) se `Some`, senão do
/// primeiro elemento.
fn join_display(vec: u64, start: i64, seed: Option<u64>) -> u64 {
    let n = vec_len(vec);
    let space = str_new(b" ");
    let mut acc: Option<u64> = seed;
    let mut i = start;
    while i < n {
        let h = vec_get(vec, i) as u64;
        acc = Some(match acc {
            None => h,
            Some(prev) => str_concat(str_concat(prev, space), h),
        });
        i += 1;
    }
    acc.unwrap_or_else(|| str_new(b""))
}

fn print_handle(msg: u64, is_err: bool) {
    let ptr = crate::collector::string_pool::__RTS_FN_NS_GC_STRING_PTR(msg);
    let len = crate::collector::string_pool::__RTS_FN_NS_GC_STRING_LEN(msg);
    let p = ptr as *const u8;
    if is_err {
        crate::io::__RTS_FN_NS_IO_EPRINT(p, len);
    } else {
        crate::io::__RTS_FN_NS_IO_PRINT(p, len);
    }
}

/// `console.<method>(...)` drenado. `display_vec` = handles já coergidos p/
/// exibição (join no runtime); `raw_vec` = valores crus (override/assert).
/// Despacha override runtime se houver; senão imprime no fd do método.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_CONSOLE_WRITE_AUTO(
    method_ptr: *const u8,
    method_len: i64,
    display_vec: u64,
    raw_vec: u64,
) -> i64 {
    let method = read_method(method_ptr, method_len);

    // 1) Override runtime: `(console as any).m = fn`. Variádico empacota raw_vec
    //    dentro de [raw_vec]; senão passa raw_vec direto (= os args individuais).
    let (override_h, is_var) = CONSOLE_OVERRIDES
        .with(|c| c.borrow().get(&method).copied())
        .unwrap_or((0, false));
    if override_h != 0 {
        let args = if is_var {
            let wrapped = rts_shared::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            rts_shared::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(wrapped, raw_vec as i64);
            wrapped
        } else {
            raw_vec
        };
        return rts_primitives::function::ops::__RTS_FN_RT_INVOKE_AUTO(override_h, 0, args);
    }

    // 2) Nativo, por método.
    match method.as_str() {
        // assert(cond, ...msg): cond truthy = noop; senão "Assertion failed: msg".
        "assert" => {
            if vec_len(raw_vec) == 0 {
                return 0;
            }
            let cond = vec_get(raw_vec, 0);
            if crate::collector::string_pool::__RTS_FN_RT_TRUTHY(cond) != 0 {
                return 0;
            }
            let prefix = str_new(b"Assertion failed:");
            let msg = join_display(display_vec, 1, Some(prefix));
            print_handle(msg, true);
        }
        "error" | "warn" => {
            let msg = join_display(display_vec, 0, None);
            print_handle(msg, true);
        }
        // log/info/debug/dir + métodos noop (table/group/count/trace/time*/...)
        // imprimem o join em stdout (paridade com o comportamento atual).
        _ => {
            let msg = join_display(display_vec, 0, None);
            print_handle(msg, false);
        }
    }
    0
}
