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
    // (cross-runtime #799) return_kind=4 (void) cai pro caminho i64 —
    // o retorno e' descartado pelo caller e a Win64 fastcall ABI nao
    // tem prologue diferente entre `() -> i64` e `()` para essa
    // aridade. Sem esta normalizacao, fns void (constructors) caem
    // no fallback invoke_all_i64 que ignora param_kinds (f64 args
    // chegam como i64 bits → NaN no body).
    let return_kind = if return_kind == 4 { 0 } else { return_kind };
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

/// Invoca `fn_ptr` (todos os params i64 -> i64) com aridade ate 16. As fns
/// geradas pelo codegen (user fns address-taken, arrows liftadas) usam
/// `default_call_conv`, que coincide com `extern "C"` da plataforma.
///
/// (#1281) PORTAVEL por construcao: `transmute` para `extern "C" fn(i64..) ->
/// i64` deixa o compilador gerar a sequencia de chamada correta para a ABI de
/// CADA alvo (Win64 / System V / AArch64) — sem asm por-plataforma (que era
/// Win64-only e panicava >8 em Linux/Mac, #1287) nem dependencia C (libffi-sys
/// nao compila em macOS/Windows, #1287). Teto de 16 cobre curry profundo real;
/// acima disso, erro claro (raro — currying de 16+ niveis nao ocorre na pratica).
unsafe fn invoke_all_i64(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(a(0)),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(a(0), a(1)),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2)),
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3)),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4)),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5)),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6)),
            8 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7)),
            9 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8)),
            10 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9)),
            11 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10)),
            12 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10), a(11)),
            13 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10), a(11), a(12)),
            14 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10), a(11), a(12), a(13)),
            15 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10), a(11), a(12), a(13), a(14)),
            16 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9), a(10), a(11), a(12), a(13), a(14), a(15)),
            n => panic!("invoke_all_i64: aridade {n} > 16 nao suportada (curry/fn extremamente profundo)"),
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

/// (#195) Invoca um callback de array method que pode ser um Entry::Function
/// (com bound_args de captura) OU um fn_ptr cru. `extra` sao os args reais
/// passados pelo runtime do array method (ex: [val, idx]). Quando handle eh
/// Function, monta `bound_args ++ extra` e usa invoke_typed (param_kinds +
/// return_kind corretos) — semantica de captura por-ativacao. Quando eh
/// fn_ptr cru (sem captura), chama direto via invoke_all_i64 ignorando o
/// terceiro slot (array handle) que callbacks 1-2 param descartam.
pub(crate) fn invoke_array_callback(handle_or_ptr: u64, extra: &[i64]) -> i64 {
    if let Some((fn_ptr, bound, _hbt, _bt, _arrow, _htp, param_kinds, ret_kind)) =
        read_function_data(handle_or_ptr)
    {
        let mut all: Vec<i64> = Vec::with_capacity(bound.len() + extra.len());
        all.extend_from_slice(&bound);
        all.extend_from_slice(extra);
        unsafe { invoke_typed(fn_ptr, &all, &param_kinds, ret_kind) }
    } else {
        // fn_ptr cru. Callbacks de array method recebem (val, idx, arr).
        unsafe { invoke_all_i64(handle_or_ptr, extra) }
    }
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

/// (#195) Reifica uma arrow liftada com variaveis CAPTURADAS por valor.
/// `bound_args_handle` aponta um `Entry::Vec` com os valores das capturas, na
/// mesma ordem em que aparecem como params INICIAIS da fn liftada. Em cada
/// invocacao, FUNCTION_CALL/INVOKE_AUTO fazem `all_args = bound_args ++
/// args_reais` — semantica de captura-por-valor por-ativacao (curry/recursao
/// corretos, ao contrario do antigo promote-to-global compartilhado).
///
/// `param_kinds_ptr/len` descreve TODOS os params (capturas + proprios) pra
/// que `invoke_typed` reinterprete f64-bits corretamente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_REIFY_CAPTURED(
    fn_ptr: u64,
    arity: i64,
    name_ptr: i64,
    name_len: i64,
    is_arrow: i32,
    has_this_param: i32,
    bound_args_handle: u64,
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
    let bound_args = read_args_vec(bound_args_handle);
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
        bound_this: 0,
        has_bound_this: false,
        bound_args,
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
    // (#218 phase2) Proxy callable: redireciona pra trap apply ou forward.
    if let Some((target, handler)) =
        crate::namespaces::globals::proxy::ops::resolve_proxy(handle)
    {
        return crate::namespaces::globals::proxy::ops::dispatch_apply(
            target,
            handler,
            this_arg,
            args_handle,
        );
    }
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
    } else if has_bound_this {
        // Arrow com `this` capturado em criação (REIFY_BOUND). Empurra ao
        // slot para que THIS_GET() no body leia o valor correto mesmo quando
        // a arrow é chamada fora do escopo original.
        crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(bound_this);
        true
    } else {
        false
    };

    // Args ja' vem encoded corretamente do codegen (numbers como f64 bits
    // via bitcast). Reflect.apply faz conversao int->f64 antes de chamar
    // FUNCTION_CALL.
    let typed_args = all_args.clone();
    // (cross-runtime #799) Variadic fold: callee binario f64 (ex:
    // `Math.max(a,b)`) chamado via `Reflect.apply(Math.max, null, [...n])`
    // com N > 2 args — RTS nao tem variadic na ABI, mas Math.max/min sao
    // semanticamente reduce f64. Quando param_kinds=[1,1] e return=1 e
    // args.len() > 2, foldamos via chamadas sucessivas.
    let result = if param_kinds.len() == 2
        && param_kinds == vec![1u8, 1u8]
        && return_kind == 1
        && typed_args.len() > 2
    {
        // typed_args ja' tem todos os elementos convertidos pra f64 bits
        // (acima); o fold preserva isso porque invoke_typed retorna f64
        // bits e ja' acc/x sao bits.
        let mut acc = typed_args[0];
        for &x in &typed_args[1..] {
            acc = unsafe { invoke_typed(fn_ptr, &[acc, x], &[1u8, 1u8], 1) };
        }
        acc
    } else {
        unsafe { invoke_typed(fn_ptr, &typed_args, &param_kinds, return_kind) }
    };

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

/// (cross-runtime #799) Reflect.apply/construct usam essa variante que
/// converte args int->f64 bits antes de chamar CALL, baseado em
/// param_kinds do callee. Vec<i64> de fixture (`[10, 20]`) tem numbers
/// como i64 puros — sem essa conversao, invoke_typed os interpretaria
/// como bits denormal f64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_APPLY_TYPED(
    handle: u64,
    this_arg: i64,
    args_handle: u64,
) -> i64 {
    let (param_kinds, has_this_param, is_arrow) = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            (d.param_kinds.clone(), d.has_this_param, d.is_arrow)
        } else {
            (Vec::new(), false, false)
        }
    });
    if param_kinds.is_empty() {
        return __RTS_FN_GL_FUNCTION_CALL(handle, this_arg, args_handle);
    }
    let raw_args = read_args_vec(args_handle);
    let offset = if !is_arrow && has_this_param { 1 } else { 0 };
    let last_kind = param_kinds.last().copied().unwrap_or(0);
    let converted: Vec<i64> = raw_args
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let pk = param_kinds.get(i + offset).copied().unwrap_or(last_kind);
            if pk == 1 {
                f64_to_i64(v as f64)
            } else {
                v
            }
        })
        .collect();
    // (cross-runtime #799) Variadic fold: callee binario f64 (Math.max/min)
    // chamado com N > 2 args — JS spec exige varargs; RTS ABI binaria nao
    // tem como expressar diretamente. Foldamos via chamadas sucessivas.
    if param_kinds.len() == 2
        && param_kinds == vec![1u8, 1u8]
        && converted.len() > 2
    {
        let mut acc = converted[0];
        for &x in &converted[1..] {
            let single = alloc_entry(Entry::Vec(Box::new(vec![acc, x])));
            acc = __RTS_FN_GL_FUNCTION_CALL(handle, this_arg, single);
        }
        return acc;
    }
    let new_handle = alloc_entry(Entry::Vec(Box::new(converted)));
    __RTS_FN_GL_FUNCTION_CALL(handle, this_arg, new_handle)
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

    // (cross-runtime #49) Converte args int->f64.to_bits baseado em
    // param_kinds antes de armazenar — bound_args fica em formato
    // consistente com APPLY_TYPED, que invoke_typed depois interpreta
    // como bits f64 quando pk[i]==1. Sem isso, `add.bind(null, 5)`
    // armazenava `5` como i64 puro e invoke_typed via como denormal.
    let new_args = read_args_vec(args_handle);
    let prev_n = bound_args.len();
    let offset = if !is_arrow && has_this_param { 1 } else { 0 };
    let last_kind = param_kinds.last().copied().unwrap_or(0);
    for (i, &v) in new_args.iter().enumerate() {
        let pk_idx = i + offset + prev_n;
        let pk = param_kinds.get(pk_idx).copied().unwrap_or(last_kind);
        let converted = if pk == 1 { f64_to_i64(v as f64) } else { v };
        bound_args.push(converted);
    }

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

/// (cross-runtime #336) Object.prototype singleton — Map cacheado com
/// `constructor: {name: "Object"}`. Usado em prototype chain inspection:
/// quando uma classe raiz (sem super) e' criada, seu proto Map recebe
/// `__proto__ = OBJECT_PROTOTYPE_HANDLE()`, garantindo que iteracao
/// chain `while (proto) { ... proto = Object.getPrototypeOf(proto); }`
/// termine com `proto.constructor.name === "Object"` antes do null.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE() -> u64 {
    static SINGLETON: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *SINGLETON.get_or_init(|| {
        let proto = crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_NEW();
        let ctor_stub = alloc_entry(Entry::Function(Box::new(crate::namespaces::gc::handles::FunctionData {
            fn_ptr: 0,
            arity: 0,
            name: "Object".into(),
            bound_this: 0,
            has_bound_this: false,
            bound_args: Vec::new(),
            is_arrow: false,
            has_this_param: false,
            param_kinds: Vec::new(),
            return_kind: 0,
            source: None,
            keep_alive: None,
            prototype_handle: 0,
        })));
        crate::namespaces::collections::map::with_map_mut(proto, (), |m| {
            m.insert("constructor".to_string(), ctor_stub as i64);
        });
        // (cross-runtime #377) constructor eh non-enumerable em Object.prototype.
        crate::namespaces::collections::map::mark_non_enumerable(proto, "constructor");
        proto
    })
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
    // (cross-runtime #336) Popula `constructor` slot no prototype Map.
    // JS spec: `C.prototype.constructor === C`. Sem isso,
    // `Object.getPrototypeOf(c).constructor` retorna 0 e iteracao do
    // prototype chain (`while (proto) { chain.push(proto.constructor.name); }`)
    // nao consegue extrair nomes das classes da hierarquia.
    crate::namespaces::collections::map::with_map_mut(new_proto, (), |m| {
        m.insert("constructor".to_string(), handle as i64);
    });
    // (cross-runtime #377) `constructor` slot eh non-enumerable em JS spec
    // — class methods (incluindo constructor sintetico) nao aparecem em
    // `for...in`. Sem isso, fixture 377_for_in_detail reportava
    // `x,constructor` em vez de so' `x`.
    crate::namespaces::collections::map::mark_non_enumerable(new_proto, "constructor");
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

/// (cross-runtime #387) `instance instanceof Ctor` para FUNCAO-CONSTRUTORA
/// (pre-ES6). Semantica JS: anda a `__proto__` chain de `instance` e
/// retorna true se algum elo for identico a `Ctor.prototype` (`ctor_h` eh o
/// handle Function da fn-construtora). Cobre heranca via
/// `Dog.prototype = Object.create(Animal.prototype)`: a chain da instancia
/// passa por Dog.prototype e Animal.prototype, entao `d instanceof Animal`
/// tambem casa. Retorna 0/1 (bool sentinel decidido pelo codegen).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INSTANCEOF_PROTO(instance_h: u64, ctor_h: u64) -> i64 {
    use crate::namespaces::collections::map::with_map_mut;
    // Resolve Ctor.prototype (lazy-aloca se preciso — mesma fn que `new` usa).
    let target_proto = __RTS_FN_GL_FUNCTION_PROTOTYPE_GET(ctor_h);
    if target_proto == 0 {
        return 0;
    }
    let read_proto = |h: u64| -> i64 {
        with_map_mut(h, 0i64, |m| m.get("__proto__").copied().unwrap_or(0))
    };
    // Anda a __proto__ chain da instancia.
    let mut current = read_proto(instance_h);
    let mut depth = 0u32;
    while current != 0 && depth < 64 {
        if current as u64 == target_proto {
            return 1;
        }
        current = read_proto(current as u64);
        depth += 1;
    }
    0
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
    invoke_auto_impl(callee, this_arg, args_handle, None)
}

/// (#1078/#341) INVOKE_AUTO com return_kind explicito do call site.
/// `override_return_kind`: 255 = usar o do handle; 0/1/2/3 = forcar
/// i64/f64/bool/i32. O override so' eh aplicado quando o handle nao tem
/// return_kind proprio (rk=0). Usado quando o codegen sabe o tipo de retorno
/// do metodo de prototype (ex: `circ.area(): number` via Math.sqrt) — sem
/// isso o trampolim invoca como `-> i64` e trunca o f64 (le RAX/XMM0 errado).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INVOKE_AUTO_TYPED(
    callee: i64,
    this_arg: i64,
    args_handle: u64,
    override_return_kind: i32,
) -> i64 {
    let ov = if override_return_kind == 255 {
        None
    } else {
        Some(override_return_kind as u8)
    };
    invoke_auto_impl(callee, this_arg, args_handle, ov)
}

/// (issue-pai invoke/param_kinds) Invoca o callable e NORMALIZA o retorno para
/// f64-bits, qualquer que seja o return_kind real. Resolve HOF: `apply(f, x)`
/// onde `f: (n)=>number` — o callee pode ser user fn f64-ret (handle, retorna
/// bits f64), OU function expression i64-ret (fn_ptr raw, retorna int). Esta fn
/// unifica: f64-ret -> bits ja' corretos; i64-ret -> converte (i as f64) e
/// devolve os bits. O codegen sempre bitcast o resultado p/ f64. Sem isto, o
/// mesmo `f(x)` no body de `apply` nao saberia tratar os dois callees.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INVOKE_AUTO_AS_F64(
    callee: i64,
    this_arg: i64,
    args_handle: u64,
) -> i64 {
    // Handle Function declara param_kinds/return_kind; fn_ptr raw (function
    // expression hoistada) eh i64-ABI sem kinds.
    let fdata = read_function_data(callee as u64);
    let is_handle = fdata.is_some();
    let rk = fdata
        .map(|(_, _, _, _, _, _, _, return_kind)| return_kind)
        .unwrap_or(0);
    if !is_handle {
        // fn_ptr raw i64-ABI: o caller empacotou os args number como bits f64.
        // Converte cada arg de bits-f64 -> i64 truncado antes de invocar, p/
        // que a fn i64-param (function expression) receba o numero correto.
        // Depois converte o retorno i64 -> bits f64 (normalizacao).
        let conv = convert_f64bits_args_to_i64(args_handle);
        let r = invoke_auto_impl(callee, this_arg, conv, None);
        return f64_to_i64(r as f64);
    }
    let r = invoke_auto_impl(callee, this_arg, args_handle, None);
    if rk == 1 {
        // Handle f64-ret ja' retorna bits f64 — passa direto.
        r
    } else {
        // Handle i64-ret/bool: converte p/ f64 e devolve os bits.
        f64_to_i64(r as f64)
    }
}

/// (issue-pai) Para invoke de fn_ptr raw i64-ABI: os args number vieram como
/// bits-f64 (convencao do call site dinamico). Aloca um novo args Vec com cada
/// elemento convertido de bits-f64 -> i64 truncado, p/ a fn i64-param ler o
/// valor inteiro correto. Handles/sentinels (fora do range f64 finito comum)
/// passam direto.
fn convert_f64bits_args_to_i64(args_handle: u64) -> u64 {
    let args = read_args_vec(args_handle);
    let conv: Vec<i64> = args
        .iter()
        .map(|&a| {
            let f = i64_to_f64(a);
            // So' converte quando parece um number f64 finito "limpo"; handles
            // (valores grandes/NaN bits) passam crus.
            if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007e15 {
                f as i64
            } else {
                a
            }
        })
        .collect();
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(conv)),
    )
}

fn invoke_auto_impl(
    callee: i64,
    this_arg: i64,
    args_handle: u64,
    override_return_kind: Option<u8>,
) -> i64 {
    // (#218 phase2) Proxy callable: se callee for Entry::Proxy, despacha
    // pra trap `apply` ou faz forward chamando o target via Function.apply.
    if let Some((target, handler)) =
        crate::namespaces::globals::proxy::ops::resolve_proxy(callee as u64)
    {
        return crate::namespaces::globals::proxy::ops::dispatch_apply(
            target,
            handler,
            this_arg,
            args_handle,
        );
    }
    // Tenta como handle Function primeiro.
    if let Some((fn_ptr, bound_args, has_bound_this, bound_this, is_arrow, has_this_param, param_kinds, return_kind)) =
        read_function_data(callee as u64)
    {
        let mut all_args = bound_args;
        all_args.extend(read_args_vec(args_handle));
        // (engine multi-thread passivo) Preenche args faltantes com 0
        // ate atender o numero de param_kinds. Caso: setTimeout(resolveFn, ms)
        // chama com 0 args, mas resolveFn tem param_kinds=[0,0] (promise_h
        // + value). Sem preencher, o trampolim lê lixo do stack como value.
        while all_args.len() < param_kinds.len() {
            all_args.push(0);
        }
        let pushed_this = if !is_arrow && has_this_param {
            let effective = if has_bound_this { bound_this } else { this_arg };
            all_args.insert(0, effective);
            false
        } else if !is_arrow {
            let effective = if has_bound_this { bound_this } else { this_arg };
            crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(effective);
            true
        } else if has_bound_this {
            // Arrow com `this` capturado em criação (REIFY_BOUND). Empurra ao
            // slot para que THIS_GET() no body leia o valor correto mesmo quando
            // a arrow é chamada fora do escopo original.
            crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(bound_this);
            true
        } else {
            false
        };
        // Override do call site so' quando o handle nao declara return_kind.
        let effective_rk = match override_return_kind {
            Some(ov) if return_kind == 0 => ov,
            _ => return_kind,
        };
        let r = unsafe { invoke_typed(fn_ptr, &all_args, &param_kinds, effective_rk) };
        if pushed_this {
            crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_POP();
        }
        return r;
    }
    // Fn ptr raw — usa invoke_typed com override (param_kinds vazio).
    let args_v = read_args_vec(args_handle);
    crate::namespaces::gc::this_slot::__RTS_FN_RT_THIS_PUSH(this_arg);
    let rk = override_return_kind.unwrap_or(0);
    let r = if rk == 0 {
        unsafe { invoke_n(callee as u64, &args_v) }
    } else {
        unsafe { invoke_typed(callee as u64, &args_v, &[], rk) }
    };
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
