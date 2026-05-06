//! Function — extern "C" implementations dos metodos da Function class (#359).
//!
//! Trampolim invoke_n: faz transmute do fn_ptr pra extern "C" fn(i64...) -> i64
//! por aridade ate 8. Funciona porque user fns com address taken usam
//! default_call_conv (SystemV/Win64), igual extern "C" Rust.

use std::sync::{Arc, Mutex, OnceLock};
use crate::namespaces::gc::handles::{Entry, FunctionData, alloc_entry, with_entry, with_entry_mut};

pub struct CompiledFn {
    pub fn_ptr: u64,
    pub arity: u8,
    pub keep_alive: Arc<Mutex<dyn std::any::Any + Send>>,
}

type CompileHook = fn(params: &[&str], body: &str) -> anyhow::Result<CompiledFn>;

static COMPILE_FN_HOOK: OnceLock<CompileHook> = OnceLock::new();

pub fn register_compile_fn(f: CompileHook) {
    let _ = COMPILE_FN_HOOK.set(f);
}

unsafe fn invoke_n(fn_ptr: u64, args: &[i64]) -> i64 {
    unsafe { invoke_typed(fn_ptr, args, &[], 0) }
}

/// Despacha por signature heterogênea. `param_kinds[i]` codifica:
/// 0=i64, 1=f64, 2=bool, 3=i32. `return_kind` idem (4=void → retorna 0).
/// Args entram como `i64` (representação carregada pelo handle Function);
/// se o tipo for f64/i32/bool, faz transmute/cast no boundary.
///
/// Suporta combinações comuns para métodos de classe RTS:
/// `(i64, ...mixed) -> mixed`. Assume primeiro param i64 (this) quando
/// has_this_param dispara em CALL.
unsafe fn invoke_typed(
    fn_ptr: u64,
    args: &[i64],
    param_kinds: &[u8],
    return_kind: u8,
) -> i64 {
    use std::mem::transmute;
    // Quando param_kinds é vazio, todos os args são i64 (caminho rápido).
    if param_kinds.is_empty() && return_kind == 0 {
        return unsafe { invoke_all_i64(fn_ptr, args) };
    }
    // Caminho tipado: aridade ate 4 + (this i64) — cobre métodos de classe
    // RTS comuns. Aumentar conforme necessário.
    // Convenção: param_kinds[i] descreve args[i] (this incluso se for o caso).
    let n = args.len();
    // Coerções por param.
    let a0_i64 = args.first().copied().unwrap_or(0);
    let a1_i64 = args.get(1).copied().unwrap_or(0);
    let a2_i64 = args.get(2).copied().unwrap_or(0);
    let a3_i64 = args.get(3).copied().unwrap_or(0);
    let a4_i64 = args.get(4).copied().unwrap_or(0);

    let a0_f64 = i64_to_f64(a0_i64);
    let a1_f64 = i64_to_f64(a1_i64);
    let a2_f64 = i64_to_f64(a2_i64);
    let a3_f64 = i64_to_f64(a3_i64);
    let _a4_f64 = i64_to_f64(a4_i64);

    let pk0 = param_kinds.first().copied().unwrap_or(0);
    let pk1 = param_kinds.get(1).copied().unwrap_or(0);
    let pk2 = param_kinds.get(2).copied().unwrap_or(0);
    let pk3 = param_kinds.get(3).copied().unwrap_or(0);
    let pk4 = param_kinds.get(4).copied().unwrap_or(0);

    // Encode a tupla (n, pk0, pk1, ..., return_kind) num discriminador.
    // Hot path: this (i64) + 1-3 args mistos + retorno mixed.
    unsafe {
        match (n, pk0, pk1, pk2, return_kind) {
            // 0 args, retorno mixed
            (0, _, _, _, 0) => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            (0, _, _, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn() -> f64>(fn_ptr)()),
            // 1 arg
            (1, 0, _, _, 0) => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(a0_i64),
            (1, 0, _, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn(i64) -> f64>(fn_ptr)(a0_i64)),
            (1, 1, _, _, 0) => transmute::<u64, extern "C" fn(f64) -> i64>(fn_ptr)(a0_f64),
            (1, 1, _, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn(f64) -> f64>(fn_ptr)(a0_f64)),
            // 2 args (this i64 + 1 outro)
            (2, 0, 0, _, 0) => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(a0_i64, a1_i64),
            (2, 0, 0, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn(i64, i64) -> f64>(fn_ptr)(a0_i64, a1_i64)),
            (2, 0, 1, _, 0) => transmute::<u64, extern "C" fn(i64, f64) -> i64>(fn_ptr)(a0_i64, a1_f64),
            (2, 0, 1, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn(i64, f64) -> f64>(fn_ptr)(a0_i64, a1_f64)),
            (2, 1, 1, _, 1) => f64_to_i64(transmute::<u64, extern "C" fn(f64, f64) -> f64>(fn_ptr)(a0_f64, a1_f64)),
            // 3 args (this + 2 outros)
            (3, 0, 0, 0, 0) => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(a0_i64, a1_i64, a2_i64),
            (3, 0, 1, 1, 1) => f64_to_i64(transmute::<u64, extern "C" fn(i64, f64, f64) -> f64>(fn_ptr)(a0_i64, a1_f64, a2_f64)),
            (3, 0, 0, 1, 0) => transmute::<u64, extern "C" fn(i64, i64, f64) -> i64>(fn_ptr)(a0_i64, a1_i64, a2_f64),
            (3, 0, 1, 0, 0) => transmute::<u64, extern "C" fn(i64, f64, i64) -> i64>(fn_ptr)(a0_i64, a1_f64, a2_i64),
            (3, 0, 1, 1, 0) => transmute::<u64, extern "C" fn(i64, f64, f64) -> i64>(fn_ptr)(a0_i64, a1_f64, a2_f64),
            // 4 args (this + 3 outros) — só combinações importantes
            (4, 0, 1, 1, 1) => f64_to_i64(transmute::<u64, extern "C" fn(i64, f64, f64, f64) -> f64>(fn_ptr)(a0_i64, a1_f64, a2_f64, a3_f64)),
            // Fallback: tudo i64 (perda de precisão se f64 esperado).
            _ => {
                let _ = (pk3, pk4, a3_i64, a4_i64);
                invoke_all_i64(fn_ptr, args)
            }
        }
    }
}

unsafe fn invoke_all_i64(fn_ptr: u64, args: &[i64]) -> i64 {
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

#[inline]
fn i64_to_f64(v: i64) -> f64 {
    f64::from_bits(v as u64)
}

#[inline]
fn f64_to_i64(v: f64) -> i64 {
    v.to_bits() as i64
}

fn read_function_data(handle: u64) -> Option<(u64, Vec<i64>, bool, i64, bool, bool, Vec<u8>, u8)> {
    with_entry(handle, |entry| {
        if let Some(Entry::Function(data)) = entry {
            Some((
                data.fn_ptr,
                data.bound_args.clone(),
                data.has_bound_this,
                data.bound_this,
                data.is_arrow,
                data.has_this_param,
                data.param_kinds.clone(),
                data.return_kind,
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
    has_this_param: i32,
) -> u64 {
    __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED(
        fn_ptr, arity, name_ptr, name_len, is_arrow, has_this_param, 0, 0, 0, 0, 0,
    )
}

/// REIFY com bound_this. Usado para reificação de método de instância:
/// `c.add` em posição de valor → handle Function pré-bindado em `c`.
/// Quando `bind_this != 0`, o handle resultante tem `has_bound_this=true`
/// e `bound_this=bind_this`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_REIFY_BOUND(
    fn_ptr: u64,
    arity: i64,
    name_ptr: i64,
    name_len: i64,
    is_arrow: i32,
    has_this_param: i32,
    bound_this: i64,
    has_bound_this: i32,
) -> u64 {
    __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED(
        fn_ptr, arity, name_ptr, name_len, is_arrow, has_this_param,
        bound_this, has_bound_this, 0, 0, 0,
    )
}

/// REIFY_BOUND com signature ABI (`param_kinds_ptr/len`, `return_kind`).
/// Usado para reificação de método de classe com tipos não-i64.
/// `param_kinds` codifica cada param: 0=i64, 1=f64, 2=bool, 3=i32 (este
/// codifica o param já incluindo `this` quando aplicável).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED(
    fn_ptr: u64,
    arity: i64,
    name_ptr: i64,
    name_len: i64,
    is_arrow: i32,
    has_this_param: i32,
    bound_this: i64,
    has_bound_this: i32,
    param_kinds_ptr: i64,
    param_kinds_len: i64,
    return_kind: i32,
) -> u64 {
    let name = if name_ptr != 0 && name_len > 0 {
        unsafe {
            let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len as usize);
            std::str::from_utf8(bytes).unwrap_or("anonymous").to_owned()
        }
    } else {
        "anonymous".to_owned()
    };
    let param_kinds = if param_kinds_ptr != 0 && param_kinds_len > 0 {
        unsafe {
            std::slice::from_raw_parts(param_kinds_ptr as *const u8, param_kinds_len as usize)
                .to_vec()
        }
    } else {
        Vec::new()
    };
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity: arity.clamp(0, 255) as u8,
        name: name.into_boxed_str(),
        bound_this,
        has_bound_this: has_bound_this != 0,
        bound_args: Vec::new(),
        is_arrow: is_arrow != 0,
        has_this_param: has_this_param != 0,
        param_kinds,
        return_kind: return_kind.clamp(0, 4) as u8,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
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

    let compile_fn = match COMPILE_FN_HOOK.get() {
        Some(f) => f,
        None => {
            eprintln!("new Function: compile hook not registered");
            return 0;
        }
    };
    let compiled = match compile_fn(&params, &body_str) {
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
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        source: Some(format!("function anonymous({}) {{\n{}\n}}", params_str, body_str).into_boxed_str()),
        keep_alive: Some(compiled.keep_alive),
        prototype_handle: 0,
    })))
}

/// `fn.call(thisArg, argsVec)`. Args vem como Vec handle (codegen empacota).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64 {
    let (fn_ptr, bound_args, has_bound_this, bound_this, is_arrow, has_this_param, param_kinds, return_kind) =
        match read_function_data(handle) {
            Some(d) => d,
            None => return 0,
        };

    let mut all_args = bound_args;
    all_args.extend(read_args_vec(args_handle));

    // Arrow ignora thisArg (lexical this). Non-arrow com has_this_param
    // (método de classe compilado com `this` como primeiro parâmetro)
    // recebe effective_this prepended.
    let effective_this = if has_bound_this { bound_this } else { this_arg };
    let pushed_this_slot = if !is_arrow && has_this_param {
        all_args.insert(0, effective_this);
        false
    } else if !is_arrow {
        // Plain fn (nao-classe): empilha thisArg no thread-local slot
        // pra que `Expr::This` no body leia via __RTS_FN_RT_THIS_GET.
        crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(effective_this);
        true
    } else {
        false
    };

    let result = unsafe { invoke_typed(fn_ptr, &all_args, &param_kinds, return_kind) };

    if pushed_this_slot {
        crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_POP();
    }

    result
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
                d.has_this_param,
                d.param_kinds.clone(),
                d.return_kind,
                d.source.clone(),
                d.keep_alive.clone(),
                d.has_bound_this,
                d.bound_this,
            ))
        } else {
            None
        }
    });
    let Some((fn_ptr, arity, name, mut bound_args, is_arrow, has_this_param, param_kinds, return_kind, source, keep_alive, had_bound_this, prev_bound_this)) =
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
        has_this_param,
        param_kinds,
        return_kind,
        source,
        keep_alive,
        prototype_handle: 0,
    })))
}

/// (#264) Registry global `fn_ptr → prototype_handle`. Indexa pelo
/// endereco de codigo da user fn (estavel ao longo da execucao do JIT).
/// Necessario porque cada `Animal.prototype` no codigo TS cria nova
/// Entry::Function via REIFY — armazenar prototype dentro da Entry
/// faria cada acesso retornar Map diferente.
static FN_PROTOTYPE_REGISTRY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, u64>>,
> = std::sync::OnceLock::new();

fn proto_registry() -> &'static std::sync::Mutex<std::collections::HashMap<u64, u64>> {
    FN_PROTOTYPE_REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// (#264) Lazy-aloca e retorna o handle de `fn.prototype`.
/// Constructor functions usam isto pra anexar metodos compartilhados.
/// Primeiro acesso aloca um Map vazio; chamadas subsequentes retornam o mesmo
/// (indexado pelo fn_ptr da Function entry, estavel entre REIFYs).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_PROTOTYPE_GET(handle: u64) -> u64 {
    let fn_ptr = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            Some(d.fn_ptr)
        } else {
            None
        }
    });
    let fn_ptr = match fn_ptr {
        Some(p) => p,
        None => return 0,
    };
    // Caminho rapido: ja existe.
    {
        let registry = proto_registry().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&h) = registry.get(&fn_ptr) {
            return h;
        }
    }
    // Aloca novo Map FORA do lock pra evitar reentrant locks com shards.
    let new_proto = crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_NEW();
    // Insere; se outra thread venceu a corrida, descarta o nosso.
    let mut registry = proto_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&existing) = registry.get(&fn_ptr) {
        drop(registry);
        let _ = crate::namespaces::gc::handles::free_handle(new_proto);
        return existing;
    }
    registry.insert(fn_ptr, new_proto);
    // Tambem atualiza o slot (mantido para tracing GC transitivo via mark).
    drop(registry);
    let _ = with_entry_mut(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            d.prototype_handle = new_proto;
        }
        ()
    });
    new_proto
}

/// (#proto-method) Auto-dispatch: se \`callee\` eh handle Function valido,
/// chama via invoke_typed (com return_kind correto). Senao trata como
/// fn_ptr cru e faz invoke_n (todos i64). Usado por
/// \`lower_var_member_call\` quando member access em handle pode resolver
/// para fn ptr OU para handle Function (depende se foi REIFY-ed).
///
/// `this_arg` so eh empilhado quando callee eh Function handle (caminho
/// classes). Args vem de Vec<i64> handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INVOKE_AUTO(
    callee: i64,
    this_arg: i64,
    args_handle: u64,
) -> i64 {
    // Tenta como handle Function primeiro.
    if let Some((fn_ptr, bound_args, has_bound_this, bound_this, is_arrow, has_this_param, param_kinds, return_kind)) =
        read_function_data(callee as u64)
    {
        let mut all_args = bound_args;
        all_args.extend(read_args_vec(args_handle));
        let pushed_this = if !is_arrow && has_this_param {
            let effective = if has_bound_this { bound_this } else { this_arg };
            all_args.insert(0, effective);
            false
        } else if !is_arrow {
            let effective = if has_bound_this { bound_this } else { this_arg };
            crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(effective);
            true
        } else {
            false
        };
        let r = unsafe { invoke_typed(fn_ptr, &all_args, &param_kinds, return_kind) };
        if pushed_this {
            crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_POP();
        }
        return r;
    }
    // Fn ptr raw — invoke_n com args vec.
    let args_v = read_args_vec(args_handle);
    crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(this_arg);
    let r = unsafe { invoke_n(callee as u64, &args_v) };
    crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_POP();
    r
}

/// (#264 PR4+) Substitui o prototype Map de uma user fn.
/// \`Dog.prototype = Object.create(Animal.prototype)\` precisa atualizar
/// o registry para que \`new Dog\` instale a chain correta.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_PROTOTYPE_SET(handle: u64, new_proto: u64) {
    let fn_ptr = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            Some(d.fn_ptr)
        } else {
            None
        }
    });
    let Some(fn_ptr) = fn_ptr else { return };
    let mut registry = proto_registry().lock().unwrap_or_else(|e| e.into_inner());
    registry.insert(fn_ptr, new_proto);
    drop(registry);
    // Atualiza tambem o slot da Function para tracing GC continuar valido.
    let _ = with_entry_mut(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            d.prototype_handle = new_proto;
        }
        ()
    });
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
