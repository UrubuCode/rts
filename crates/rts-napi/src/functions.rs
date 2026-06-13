//! Funções e callbacks N-API: `napi_create_function`, `napi_get_cb_info`,
//! `napi_call_function`. Ver docs/specs/napi-implementation.md (Etapa 11).
//!
//! ## Dois sentidos
//!
//! **TS chama fn nativa do addon:** `napi_create_function(cb, data)` registra o
//! `(cb, env, data)` num registry indexado pelo handle Function alocado. Quando
//! o TS invoca essa fn, o dispatch do RTS (`__RTS_FN_GL_FUNCTION_CALL` em
//! `rts-primitives`) chama o shim `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK` (impl
//! aqui, resolvido por link). O shim monta um `napi_callback_info`, chama `cb`,
//! e devolve o handle do `napi_value` retornado. Sem o registry hit, o shim
//! sinaliza "não é napi" e o dispatch segue o caminho normal.
//!
//! **Addon chama fn TS:** `napi_call_function(recv, func, argv)` empacota os
//! args num `Entry::Vec` e chama `__RTS_FN_GL_FUNCTION_CALL` (já existente).

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use rts_engine::heap::handles::{alloc_entry, Entry, FunctionData};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_callback, napi_callback_info, napi_env, napi_status, napi_value};

use napi_status::{napi_function_expected, napi_invalid_arg, napi_ok};

// Dispatch de fn TS via símbolo extern-C (resolvido no link do bin / add_fn! no
// JIT). NÃO chamamos a API Rust de `rts-primitives` diretamente porque isso
// arrastaria toda a teia de símbolos `__RTS_*` cross-crate para o link de um
// `cargo test -p rts-napi` isolado. O dispatch é validado por e2e no bin.
#[cfg(not(test))]
unsafe extern "C" {
    fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64;
}

// Stub só para o build de teste do crate isolado: fornece o símbolo (que no bin
// vem de rts-primitives) para que `cargo test -p rts-napi` linke. O dispatch
// real é coberto por e2e no bin; este stub retorna 0 e os testes do crate não
// exercitam napi_call_function.
#[cfg(test)]
unsafe fn __RTS_FN_GL_FUNCTION_CALL(_handle: u64, _this_arg: i64, _args_handle: u64) -> i64 {
    0
}

/// `(cb, env, data)` de uma fn nativa registrada por `napi_create_function`,
/// indexado pelo handle do `Entry::Function`. `env` é guardado como `usize`
/// (ponteiro opaco) por causa do `Send` do Mutex global.
struct NapiFn {
    cb: unsafe extern "C" fn(napi_env, napi_callback_info) -> napi_value,
    env: usize,
    data: *mut c_void,
}

// SAFETY: os ponteiros são opacos e só usados na thread JS que invoca a fn.
unsafe impl Send for NapiFn {}

static NAPI_CALLBACKS: Mutex<Option<HashMap<u64, NapiFn>>> = Mutex::new(None);

/// Info de chamada que o trampolim monta e `napi_get_cb_info` lê.
struct CallbackInfo {
    argv: Vec<napi_value>,
    this_arg: napi_value,
    data: *mut c_void,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_function(
    env: napi_env,
    _utf8name: *const c_char,
    _length: usize,
    cb: napi_callback,
    data: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(cb) = cb else {
        return napi_invalid_arg;
    };

    // Aloca um Entry::Function marcador. fn_ptr/packed_shim ficam 0 — a fn NÃO
    // é executada via invoke; o dispatch a intercepta pelo handle no registry.
    let handle = alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr: 0,
        arity: 0,
        name: "".into(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
    })));

    NAPI_CALLBACKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            handle,
            NapiFn {
                cb,
                env: env.0 as usize,
                data,
            },
        );

    unsafe { *result = value_from_handle(handle) };
    napi_ok
}

/// Shim chamado por `__RTS_FN_GL_FUNCTION_CALL` (rts-primitives) no início do
/// dispatch. Se `handle` é uma fn nativa N-API, executa o callback e escreve o
/// resultado (i64 = handle do napi_value) em `*out_result`, devolvendo 1. Senão
/// devolve 0 (o dispatch segue o caminho normal).
///
/// # Safety
/// `out_result` deve ser um ponteiro válido para i64.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __RTS_FN_RT_NAPI_DISPATCH_CALLBACK(
    handle: u64,
    this_arg: i64,
    args_handle: u64,
    out_result: *mut i64,
) -> i64 {
    let (cb, env_ptr, data) = {
        let guard = NAPI_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref().and_then(|m| m.get(&handle)) {
            Some(nf) => (nf.cb, nf.env, nf.data),
            None => return 0, // não é uma fn N-API
        }
    };

    // Monta os argv a partir do args_handle (Entry::Vec de handles i64).
    let argv: Vec<napi_value> = read_args(args_handle)
        .into_iter()
        .map(|h| value_from_handle(h as u64))
        .collect();

    let info = Box::new(CallbackInfo {
        argv,
        this_arg: value_from_handle(this_arg as u64),
        data,
    });
    let info_ptr = Box::into_raw(info);

    let env = napi_env(env_ptr as *mut c_void);
    let ret = unsafe { cb(env, napi_callback_info(info_ptr as *mut c_void)) };

    // Libera o CallbackInfo.
    drop(unsafe { Box::from_raw(info_ptr) });

    if !out_result.is_null() {
        unsafe { *out_result = handle_from_value(ret) as i64 };
    }
    1
}

fn read_args(args_handle: u64) -> Vec<i64> {
    if args_handle == 0 {
        return Vec::new();
    }
    rts_engine::heap::handles::with_entry(args_handle, |e| match e {
        Some(Entry::Vec(v)) => v.iter().copied().collect(),
        _ => Vec::new(),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_cb_info(
    _env: napi_env,
    cbinfo: napi_callback_info,
    argc: *mut usize,
    argv: *mut napi_value,
    this_arg: *mut napi_value,
    data: *mut *mut c_void,
) -> napi_status {
    if cbinfo.0.is_null() {
        return napi_invalid_arg;
    }
    let info = unsafe { &*(cbinfo.0 as *const CallbackInfo) };

    // `argc` é in/out: entrada = capacidade do buffer argv; saída = nº real.
    if !argc.is_null() {
        let cap = unsafe { *argc };
        let real = info.argv.len();
        if !argv.is_null() {
            let n = cap.min(real);
            for i in 0..n {
                unsafe { *argv.add(i) = info.argv[i] };
            }
            // Preenche o restante do buffer com undefined.
            for i in n..cap {
                unsafe { *argv.add(i) = value_from_handle((i64::MIN + 2) as u64) };
            }
        }
        unsafe { *argc = real };
    }
    if !this_arg.is_null() {
        unsafe { *this_arg = info.this_arg };
    }
    if !data.is_null() {
        unsafe { *data = info.data };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_function(
    _env: napi_env,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    let func_handle = handle_from_value(func);
    if func_handle == 0 {
        return napi_function_expected;
    }

    // Empacota argv num Entry::Vec (handles i64).
    let mut items: Vec<i64> = Vec::with_capacity(argc);
    if !argv.is_null() {
        for i in 0..argc {
            let v = unsafe { *argv.add(i) };
            items.push(handle_from_value(v) as i64);
        }
    }
    let args_vec = alloc_entry(Entry::Vec(Box::new(items)));

    let recv_i64 = handle_from_value(recv) as i64;
    let ret = unsafe { __RTS_FN_GL_FUNCTION_CALL(func_handle, recv_i64, args_vec) };

    if !result.is_null() {
        unsafe { *result = value_from_handle(ret as u64) };
    }
    napi_ok
}

/// Limpa o registro de uma fn N-API quando o handle é liberado. (Chamado por um
/// hook de cleanup futuro; por ora as fns vivem pelo processo, o que é seguro.)
#[allow(dead_code)]
pub fn forget_callback(handle: u64) {
    if let Ok(mut g) = NAPI_CALLBACKS.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(&handle);
        }
    }
}

// NOTA: testes do dispatch de callbacks (create_function → FUNCTION_CALL → shim
// → cb, e get_cb_info) exigem `__RTS_FN_GL_FUNCTION_CALL` (rts-primitives), que
// arrasta a teia de símbolos `__RTS_*` cross-crate — não linkável num
// `cargo test -p rts-napi` isolado. São validados por **e2e no bin**
// (`tests/napi_add_addon.test.ts`): um addon `.node` que expõe `add(a,b)`,
// chamado do TS, com o resultado comparado ao esperado. Ver
// docs/specs/napi-implementation.md (Etapa 11).
