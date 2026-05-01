//! Function — extern "C" implementations dos metodos da Function class (#359).
//!
//! Trampolim invoke_n: faz transmute do fn_ptr pra extern "C" fn(i64...) -> i64
//! por aridade ate 8. Funciona porque user fns com address taken usam
//! default_call_conv (SystemV/Win64), igual extern "C" Rust.

use crate::namespaces::gc::handles::{Entry, FunctionData, alloc_entry, with_entry};

unsafe fn invoke_n(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3],
            ),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4],
            ),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(
                fn_ptr,
            )(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            _ => 0,
        }
    }
}

fn read_function_data(handle: u64) -> Option<(u64, Vec<i64>, bool, i64, bool)> {
    with_entry(handle, |entry| {
        if let Some(Entry::Function(data)) = entry {
            Some((
                data.fn_ptr,
                data.bound_args.clone(),
                data.has_bound_this,
                data.bound_this,
                data.is_arrow,
            ))
        } else {
            None
        }
    })
}

fn read_args_vec(args_handle: u64) -> Vec<i64> {
    if args_handle == 0 {
        return Vec::new();
    }
    with_entry(args_handle, |entry| {
        if let Some(Entry::Vec(v)) = entry {
            v.iter().copied().collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    })
}

/// Reifica uma user fn estatica (fn_ptr conhecido em compile time) num
/// handle Function. Codegen emite essa call quando ve member access em
/// ident de user fn (ex: `myFn.bind(this)` ou `myFn.name`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_REIFY(
    fn_ptr: u64,
    arity: i64,
    name_ptr: i64,
    name_len: i64,
    is_arrow: i32,
) -> u64 {
    let name = if name_ptr != 0 && name_len > 0 {
        unsafe {
            let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len as usize);
            std::str::from_utf8(bytes).unwrap_or("anonymous").to_owned()
        }
    } else {
        "anonymous".to_owned()
    };
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity: arity.clamp(0, 255) as u8,
        name: name.into_boxed_str(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: is_arrow != 0,
        source: None,
        keep_alive: None,
    })))
}

/// `new Function(...args, body)` — compila body em runtime via eval.
/// Args: `params_handle` (string handle com nomes separados por virgula),
/// `body_handle` (string handle com codigo do body).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_NEW(params_handle: u64, body_handle: u64) -> u64 {
    let params_str = with_entry(params_handle, |e| {
        if let Some(Entry::String(b)) = e {
            std::str::from_utf8(b).map(|s| s.to_owned()).ok()
        } else {
            None
        }
    })
    .unwrap_or_default();
    let body_str = with_entry(body_handle, |e| {
        if let Some(Entry::String(b)) = e {
            std::str::from_utf8(b).map(|s| s.to_owned()).ok()
        } else {
            None
        }
    })
    .unwrap_or_default();

    let params: Vec<&str> = if params_str.trim().is_empty() {
        Vec::new()
    } else {
        params_str.split(',').map(|s| s.trim()).collect()
    };

    let compiled = match super::eval_compile::compile_function(&params, &body_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("new Function: erro de compilacao: {}", e);
            return 0;
        }
    };

    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr: compiled.fn_ptr,
        arity: compiled.arity,
        name: Box::from("anonymous"),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        source: Some(format!("function anonymous({}) {{\n{}\n}}", params_str, body_str).into_boxed_str()),
        keep_alive: Some(compiled.keep_alive),
    })))
}

/// `fn.call(thisArg, argsVec)`. Args vem como Vec handle (codegen empacota).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64 {
    let (fn_ptr, bound_args, has_bound_this, bound_this, is_arrow) =
        match read_function_data(handle) {
            Some(d) => d,
            None => return 0,
        };

    let mut all_args = bound_args;
    all_args.extend(read_args_vec(args_handle));

    // this binding: arrow ignora; com bound, usa bound_this; senao this_arg.
    // RTS user fns nao tem slot reservado pra this — entao nao prepende.
    // Mantemos this_arg disponivel pra extensao futura quando methods de
    // classe forem reificaveis.
    let _effective_this = if is_arrow {
        0
    } else if has_bound_this {
        bound_this
    } else {
        this_arg
    };

    unsafe { invoke_n(fn_ptr, &all_args) }
}

/// `fn.apply(thisArg, argsArray)`. Mesmo dispatch de call.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_APPLY(
    handle: u64,
    this_arg: i64,
    args_handle: u64,
) -> i64 {
    __RTS_FN_GL_FUNCTION_CALL(handle, this_arg, args_handle)
}

/// `fn.bind(thisArg, ...args)` — retorna nova Function com partial.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_BIND(handle: u64, this_arg: i64, args_handle: u64) -> u64 {
    let original = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            Some((
                d.fn_ptr,
                d.arity,
                d.name.clone(),
                d.bound_args.clone(),
                d.is_arrow,
                d.source.clone(),
                d.keep_alive.clone(),
                d.has_bound_this,
                d.bound_this,
            ))
        } else {
            None
        }
    });
    let Some((fn_ptr, arity, name, mut bound_args, is_arrow, source, keep_alive, had_bound_this, prev_bound_this)) =
        original
    else {
        return 0;
    };

    bound_args.extend(read_args_vec(args_handle));

    // Bind preserva primeiro bind feito (Node spec: re-bind nao troca thisArg).
    let (final_this, final_has) = if had_bound_this {
        (prev_bound_this, true)
    } else {
        (this_arg, true)
    };

    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity,
        name,
        bound_this: final_this,
        has_bound_this: final_has,
        bound_args,
        is_arrow,
        source,
        keep_alive,
    })))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_NAME(handle: u64) -> u64 {
    let name = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            d.name.to_string()
        } else {
            String::new()
        }
    });
    alloc_entry(Entry::String(name.into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_LENGTH(handle: u64) -> i64 {
    with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            (d.arity as i64).saturating_sub(d.bound_args.len() as i64).max(0)
        } else {
            0
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_TO_STRING(handle: u64) -> u64 {
    let s = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            if let Some(src) = &d.source {
                src.to_string()
            } else {
                format!("function {}() {{ [native code] }}", d.name)
            }
        } else {
            String::new()
        }
    });
    alloc_entry(Entry::String(s.into_bytes()))
}
