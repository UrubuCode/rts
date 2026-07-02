//! Classes nativas N-API: `napi_define_class` + `napi_new_instance`. Destrava
//! addons que expõem classes (`new addon.Foo()` + `foo.method()`), como
//! hashers stateful, DB handles, parsers. Ver docs/specs/napi-implementation.md.
//!
//! Modelo: `define_class` cria um `Entry::Function` (o construtor) e registra,
//! num mapa global indexado pelo handle do construtor, a tabela de métodos
//! (`nome → napi_callback`). `new_instance` cria a instância (um `Entry::Map`
//! marcado com o handle do construtor + os métodos), chama o construtor nativo
//! (que tipicamente faz `napi_wrap`), e devolve o Map. `obj.method(args)` é
//! roteado pelo codegen para `napi_invoke_method` (abaixo) que acha o callback.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry, FunctionData};

use crate::env::{handle_from_value, value_from_handle};
use crate::napi_property_descriptor;
use crate::types::{napi_callback, napi_callback_info, napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

/// Definição de uma classe nativa: o construtor + métodos de instância.
struct ClassDef {
    constructor: unsafe extern "C" fn(napi_env, napi_callback_info) -> napi_value,
    ctor_data: usize,
    /// nome do método → (callback, data)
    methods: HashMap<String, (unsafe extern "C" fn(napi_env, napi_callback_info) -> napi_value, usize)>,
}

// SAFETY: ponteiros opacos usados só na thread JS.
unsafe impl Send for ClassDef {}

/// handle do construtor (Entry::Function) → ClassDef.
static CLASSES: Mutex<Option<HashMap<u64, ClassDef>>> = Mutex::new(None);

/// Chave reservada no Map da instância que guarda o handle do construtor (p/
/// resolver a classe ao chamar um método).
const CLASS_KEY: &str = "__napi_class_ctor__";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_define_class(
    _env: napi_env,
    _utf8name: *const c_char,
    _length: usize,
    constructor: napi_callback,
    ctor_data: *mut c_void,
    property_count: usize,
    properties: *const napi_property_descriptor,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(ctor) = constructor else {
        return napi_invalid_arg;
    };

    // Aloca o Entry::Function marcador do construtor.
    let ctor_handle = alloc_entry(Entry::Function(Box::new(FunctionData {
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
        uniform_thunk: false,
    })));

    // Coleta os métodos de instância dos descriptors.
    let mut methods = HashMap::new();
    if !properties.is_null() {
        for i in 0..property_count {
            let desc = unsafe { &*properties.add(i) };
            if let Some(m) = desc.method {
                if let Some(name) = unsafe { cstr(desc.utf8name) } {
                    methods.insert(name, (m, desc.data as usize));
                }
            }
            // value/getter/setter de classe: Fase 2 ignora (raro em addons).
        }
    }

    CLASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            ctor_handle,
            ClassDef {
                constructor: ctor,
                ctor_data: ctor_data as usize,
                methods,
            },
        );

    unsafe { *result = value_from_handle(ctor_handle) };
    napi_ok
}

/// `new ctor(args)`: cria a instância (Map marcado), chama o construtor nativo
/// com `this` = a instância, devolve a instância.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_new_instance(
    env: napi_env,
    constructor: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let ctor_handle = handle_from_value(constructor);
    let (ctor, ctor_data) = {
        let guard = CLASSES.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref().and_then(|m| m.get(&ctor_handle)) {
            Some(c) => (c.constructor, c.ctor_data),
            None => return napi_invalid_arg,
        }
    };

    // Instância = Map marcado com o handle do construtor.
    let inst = alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new())));
    with_entry_mut(inst, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert(CLASS_KEY.to_string(), ctor_handle as i64);
        }
    });

    // Monta o callback_info do construtor com this = instância.
    let argv_slice: Vec<napi_value> = if argv.is_null() {
        Vec::new()
    } else {
        (0..argc).map(|i| unsafe { *argv.add(i) }).collect()
    };
    let ret = crate::functions::invoke_napi_callback(
        env,
        ctor,
        ctor_data as *mut c_void,
        value_from_handle(inst),
        &argv_slice,
    );

    // O construtor pode devolver outro objeto; senão usa a instância.
    let final_h = {
        let r = handle_from_value(ret);
        if r != 0 && r != (i64::MIN + 2) as u64 { r } else { inst }
    };
    // Garante que o tag de classe sobreviva (se o ctor devolveu a própria inst).
    unsafe { *result = value_from_handle(final_h) };
    napi_ok
}

/// Versão runtime-friendly de `napi_new_instance` chamada pelo codegen: recebe
/// o handle do construtor e um `Entry::Vec` de args (napi_value handles),
/// devolve o handle da instância (ou 0 se o ctor não é uma classe nativa).
///
/// # Safety
/// `ctor_handle`/`args_handle` são handles válidos da HandleTable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __RTS_FN_RT_NAPI_NEW_INSTANCE(
    ctor_handle: u64,
    args_handle: u64,
) -> u64 {
    let argv: Vec<napi_value> = read_args(args_handle)
        .into_iter()
        .map(|h| value_from_handle(h as u64))
        .collect();
    let env = napi_env(std::ptr::null_mut());
    let mut result = napi_value(std::ptr::null_mut());
    let status = unsafe {
        napi_new_instance(
            env,
            value_from_handle(ctor_handle),
            argv.len(),
            argv.as_ptr(),
            &mut result,
        )
    };
    if status == napi_ok {
        handle_from_value(result)
    } else {
        0
    }
}

/// Invoca o método `name` da instância `recv` (resolvido pela classe marcada no
/// Map). Chamado pelo codegen para `obj.method(args)` quando `obj` é instância
/// de classe nativa. Devolve o handle do resultado, ou 0 se não for método
/// nativo (o caller segue o dispatch normal).
///
/// # Safety
/// `args_ptr`/`argc` descrevem `argc` handles i64; `out` é ptr válido.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __RTS_FN_RT_NAPI_INVOKE_METHOD(
    recv: u64,
    method_ptr: *const u8,
    method_len: i64,
    args_handle: u64,
    out: *mut i64,
) -> i64 {
    if method_ptr.is_null() {
        return 0;
    }
    let name = unsafe {
        match std::str::from_utf8(std::slice::from_raw_parts(method_ptr, method_len as usize)) {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };
    // Acha o handle do construtor marcado na instância.
    let ctor_handle = with_entry(recv, |e| match e {
        Some(Entry::Map(m)) => m.get(CLASS_KEY).map(|&v| v as u64),
        _ => None,
    });
    let Some(ctor_handle) = ctor_handle else {
        return 0;
    };
    // Resolve o método na ClassDef.
    let (cb, data) = {
        let guard = CLASSES.lock().unwrap_or_else(|e| e.into_inner());
        match guard
            .as_ref()
            .and_then(|m| m.get(&ctor_handle))
            .and_then(|c| c.methods.get(name))
        {
            Some(&(cb, data)) => (cb, data),
            None => return 0,
        }
    };
    // Monta argv e invoca o método com this = recv.
    let args: Vec<napi_value> = read_args(args_handle)
        .into_iter()
        .map(|h| value_from_handle(h as u64))
        .collect();
    // env: usa o env do construtor? Para Fase 2, um env "vazio" é suficiente
    // (as fns N-API que ignoram env). Reusa um env nulo — addons que dependem do
    // env real no método são raros; melhorar é follow-up.
    let env = napi_env(std::ptr::null_mut());
    let ret = crate::functions::invoke_napi_callback(
        env,
        cb,
        data as *mut c_void,
        value_from_handle(recv),
        &args,
    );
    if !out.is_null() {
        unsafe { *out = handle_from_value(ret) as i64 };
    }
    1
}

fn read_args(args_handle: u64) -> Vec<i64> {
    if args_handle == 0 {
        return Vec::new();
    }
    with_entry(args_handle, |e| match e {
        Some(Entry::Vec(v)) => v.iter().copied().collect(),
        _ => Vec::new(),
    })
}

unsafe fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        std::str::from_utf8(std::slice::from_raw_parts(p as *const u8, len))
            .ok()
            .map(|s| s.to_string())
    }
}
