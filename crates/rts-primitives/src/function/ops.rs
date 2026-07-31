//! Function — extern "C" implementations dos metodos da Function class (#359).
//!
//! Trampolim invoke_n: faz transmute do fn_ptr pra extern "C" fn(i64...) -> i64
//! por aridade ate 8. Funciona porque user fns com address taken usam
//! default_call_conv (SystemV/Win64), igual extern "C" Rust.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry, FunctionData};
use std::sync::{Arc, Mutex, OnceLock};

// (Fase 2.3) Function é primordial e migra p/ `rts-primitives`, que NÃO pode
// depender de `rts-shared` (ciclo). As operações de prototype-map e os traps de
// Proxy callable que Function precisa ficam em `rts-shared` (collections/map +
// globals/proxy) e são chamadas por SÍMBOLO via estes shims extern-C — resolvem
// por link (AOT staticlib) ou `add_fn!` (JIT). Mesmo padrão de `gc_surface`.
unsafe extern "C" {
    fn __RTS_FN_NS_COLLECTIONS_MAP_NEW() -> u64;
    fn __RTS_FN_RT_MAP_SET_STR(map: u64, key_ptr: *const u8, key_len: i64, val: i64);
    fn __RTS_FN_RT_MAP_MARK_NON_ENUM(map: u64, key_ptr: *const u8, key_len: i64);
    fn __RTS_FN_RT_PROXY_RESOLVE(handle: u64, out_target: *mut u64, out_handler: *mut u64) -> i64;
    fn __RTS_FN_RT_PROXY_DISPATCH_APPLY(
        target: u64,
        handler: u64,
        this_arg: i64,
        args_handle: u64,
    ) -> i64;
}

/// Helper: resolve `handle` como Proxy via o shim extern-C (out-params).
#[inline]
fn proxy_resolve(handle: u64) -> Option<(u64, u64)> {
    let mut target: u64 = 0;
    let mut handler: u64 = 0;
    let is_proxy = unsafe { __RTS_FN_RT_PROXY_RESOLVE(handle, &mut target, &mut handler) };
    if is_proxy != 0 {
        Some((target, handler))
    } else {
        None
    }
}

pub struct CompiledFn {
    pub fn_ptr: u64,
    pub arity: u8,
    /// `true` quando `fn_ptr` é o THUNK de ABI uniforme do motor novo
    /// (`(env, a0..a3, rest) -> word`, PolyValue nos dois sentidos) — o
    /// invoke roteia como qualquer fn-value do motor. `false` = fn_ptr cru
    /// legado all-i64.
    pub uniform: bool,
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

/// (#1281 packed) Invoca um SHIM de assinatura unica `extern "C" fn(*const i64,
/// i64) -> i64`, passando o buffer de args (todos i64; f64 viaja como bits) e o
/// comprimento. O shim (gerado pelo codegen) desempacota e chama a fn original
/// com as coercoes corretas embutidas em IR. Aridade ARBITRARIA, portavel (a
/// assinatura do shim eh fixa — sem match por aridade, sem teto, sem asm).
///
/// O buffer `args` precisa viver durante a chamada; o shim NAO pode reter o ptr.
unsafe fn invoke_packed(shim_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    let shim = unsafe { transmute::<u64, extern "C" fn(*const i64, i64) -> i64>(shim_ptr) };
    shim(args.as_ptr(), args.len() as i64)
}

/// Despacha por signature heterogênea. `param_kinds[i]` codifica:
/// 0=i64, 1=f64, 2=bool, 3=i32. `return_kind` idem (4=void → retorna 0).
/// Args entram como `i64` (representação carregada pelo handle Function);
/// se o tipo for f64/i32/bool, faz transmute/cast no boundary.
///
/// Suporta combinações comuns para métodos de classe RTS:
/// `(i64, ...mixed) -> mixed`. Assume primeiro param i64 (this) quando
/// has_this_param dispara em CALL.
unsafe fn invoke_typed(fn_ptr: u64, args: &[i64], param_kinds: &[u8], return_kind: u8) -> i64 {
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
    let n = args.len();
    // Normaliza param_kinds para so' 0 (i64) / 1 (f64). Os kinds 2 (bool) e
    // 3 (i32) viajam como i64 na ABI Win64 — tratar como 0 e' seguro. Indices
    // sem param_kind explicito assumem i64. `nk(i)` retorna 1 sse o param i
    // eh f64.
    let nk = |i: usize| -> bool { param_kinds.get(i).copied().unwrap_or(0) == 1 };
    // return_kind normalizado: 1 = f64-ret, qualquer outro = i64-ret.
    let rk_f64 = return_kind == 1;

    // Macro declarativa: gera TODOS os 2^n combos de param_kinds (cada param
    // i64 ou f64) x 2 return_kinds, para uma dada lista de "slots" de args.
    //
    // `dispatch!(fn_ptr, rk_f64; [a0, a1, ...])` expande recursivamente: para
    // cada arg `ai` (cujo bit em `nk(i)` ja' foi computado num `bool bi`),
    // ramifica i64-vs-f64 acumulando o tipo (`$tys`) e o valor coagido
    // (`$vals`). Quando a lista de args esgota, monta o transmute
    // `extern "C" fn($tys...) -> R` exato e chama com `$vals...`, reempacotando
    // o retorno conforme `rk_f64`. Cada arm chama EXATAMENTE a assinatura que
    // casa os valores passados — sem UB.
    macro_rules! dispatch {
        // Passo recursivo: ainda restam args para peelar (token `rest`
        // contem ao menos um `(bit, raw)`). Vem ANTES do caso base para
        // resolver a ambiguidade de forma deterministica.
        (@build $fp:expr, $rk:expr; [$($ty:ty),*]; [$($val:expr),*];
                ($b:expr, $raw:expr) $(, ($bn:expr, $rawn:expr))*) => {{
            if $b {
                let f = i64_to_f64($raw);
                dispatch!(@build $fp, $rk; [$($ty,)* f64]; [$($val,)* f];
                          $(($bn, $rawn)),*)
            } else {
                let v = $raw;
                dispatch!(@build $fp, $rk; [$($ty,)* i64]; [$($val,)* v];
                          $(($bn, $rawn)),*)
            }
        }};
        // Caso base: nao restam args (nenhum token apos `;`). Monta o
        // transmute final com a assinatura exata acumulada.
        (@build $fp:expr, $rk:expr; [$($ty:ty),*]; [$($val:expr),*];) => {{
            use std::mem::transmute;
            if $rk {
                f64_to_i64(
                    transmute::<u64, extern "C" fn($($ty),*) -> f64>($fp)($($val),*)
                )
            } else {
                transmute::<u64, extern "C" fn($($ty),*) -> i64>($fp)($($val),*)
            }
        }};
        // Entrada: lista de (bit, raw) por arg. Sempre fecha com `;` para
        // que o caso base case com lista vazia de `rest` case sem ambiguidade.
        ($fp:expr, $rk:expr; $(($b:expr, $raw:expr)),*) => {{
            dispatch!(@build $fp, $rk; []; []; $(($b, $raw)),*)
        }};
    }

    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    unsafe {
        match n {
            0 => dispatch!(fn_ptr, rk_f64;),
            1 => dispatch!(fn_ptr, rk_f64; (nk(0), a(0))),
            2 => dispatch!(fn_ptr, rk_f64; (nk(0), a(0)), (nk(1), a(1))),
            3 => dispatch!(fn_ptr, rk_f64; (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2))),
            4 => dispatch!(fn_ptr, rk_f64;
                (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2)), (nk(3), a(3))),
            5 => dispatch!(fn_ptr, rk_f64;
                (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2)), (nk(3), a(3)),
                (nk(4), a(4))),
            6 => dispatch!(fn_ptr, rk_f64;
                (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2)), (nk(3), a(3)),
                (nk(4), a(4)), (nk(5), a(5))),
            7 => dispatch!(fn_ptr, rk_f64;
                (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2)), (nk(3), a(3)),
                (nk(4), a(4)), (nk(5), a(5)), (nk(6), a(6))),
            8 => dispatch!(fn_ptr, rk_f64;
                (nk(0), a(0)), (nk(1), a(1)), (nk(2), a(2)), (nk(3), a(3)),
                (nk(4), a(4)), (nk(5), a(5)), (nk(6), a(6)), (nk(7), a(7))),
            // Aridade > 8: nenhum combo tipado gerado — cai no fallback
            // i64-cru (perda de precisao para f64, mas raro na pratica).
            _ => invoke_all_i64(fn_ptr, args),
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
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
            ),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
            ),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
            ),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
            ),
            8 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(
                fn_ptr,
            )(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7)),
            9 => {
                transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64>(
                    fn_ptr,
                )(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8))
            }
            10 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8), a(9)),
            11 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
            ),
            12 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
                a(11),
            ),
            13 => transmute::<
                u64,
                extern "C" fn(
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                ) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
                a(11),
                a(12),
            ),
            14 => transmute::<
                u64,
                extern "C" fn(
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                ) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
                a(11),
                a(12),
                a(13),
            ),
            15 => transmute::<
                u64,
                extern "C" fn(
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                ) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
                a(11),
                a(12),
                a(13),
                a(14),
            ),
            16 => transmute::<
                u64,
                extern "C" fn(
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                    i64,
                ) -> i64,
            >(fn_ptr)(
                a(0),
                a(1),
                a(2),
                a(3),
                a(4),
                a(5),
                a(6),
                a(7),
                a(8),
                a(9),
                a(10),
                a(11),
                a(12),
                a(13),
                a(14),
                a(15),
            ),
            n => panic!(
                "invoke_all_i64: aridade {n} > 16 nao suportada (curry/fn extremamente profundo)"
            ),
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

/// (cross-runtime closures — function-value calling convention) Normaliza args
/// `number` para a convenção do `invoke_typed`: um param f64 (`param_kinds[i]==1`)
/// espera os BITS de um f64. Args que entram via INVOKE_AUTO/FUNCTION_CALL a
/// partir de arrays/literais/captura (ex.: `fns.reduce((v,f)=>f(v), x)`, ou
/// `(...a)=>fn(...a)`) podem chegar como INTEIRO CRU (um elemento de `[1,2,3]`
/// é o i64 `1`, não `f64::to_bits(1.0)`). Sem isto o `invoke_typed` faz
/// `from_bits(1)` ≈ 0 e a função recebe lixo (`partial(add,10)(1,2)` → 3).
///
/// Heurística que NÃO toca o caminho que já funciona: reinterpretar o i64 como
/// bits-f64 de um número "normal" dá um valor finito numa faixa sã; um inteiro
/// cru pequeno (1, 2, 42, …) reinterpreta para denormal/≈0 (fora da faixa). Só
/// reencoda quando o valor NÃO parece bits-f64 válidos — então args já
/// codificados como bits (grandes) passam intactos. Aplica-se apenas a `pk==1`,
/// logo handles/inteiros i64 (`pk==0`) nunca são alterados.
fn normalize_f64_bits_args(args: &mut [i64], param_kinds: &[u8]) {
    for (i, a) in args.iter_mut().enumerate() {
        if param_kinds.get(i).copied().unwrap_or(0) != 1 {
            continue;
        }
        let as_bits = f64::from_bits(*a as u64);
        let looks_like_f64_bits = as_bits == 0.0
            || (as_bits.is_finite() && as_bits.abs() >= 1e-200 && as_bits.abs() < 1e200);
        if !looks_like_f64_bits {
            // Parece inteiro cru → reencoda como bits f64 (o valor que o
            // invoke_typed vai `from_bits` de volta).
            *a = f64::to_bits(*a as f64) as i64;
        }
    }
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

/// Le a flag `uniform_thunk` (motor novo: fn_ptr = thunk de ABI uniforme de 6
/// slots). Fn separada p/ nao inflar o tuple de read_function_data.
fn read_uniform_thunk(handle: u64) -> bool {
    with_entry(handle, |entry| {
        if let Some(Entry::Function(d)) = entry {
            d.uniform_thunk
        } else {
            false
        }
    })
}

/// (#1281 packed) Le o packed_shim de um handle Function (0 = sem shim).
/// Funcao separada p/ nao inflar o tuple de read_function_data (8 campos,
/// usado em muitos call sites).
fn read_packed_shim(handle: u64) -> u64 {
    with_entry(handle, |entry| {
        if let Some(Entry::Function(d)) = entry {
            d.packed_shim
        } else {
            0
        }
    })
}

/// (cross-runtime #195) Indice do rest param de uma fn reificada (-1 se nao
/// variadic). Lido no invoke pra empacotar o tail antes do dispatch.
fn read_rest_param_idx(handle: u64) -> i32 {
    with_entry(handle, |entry| {
        if let Some(Entry::Function(d)) = entry {
            d.rest_param_idx
        } else {
            -1
        }
    })
}

/// (cross-runtime #195) Empacota o tail variadic. Dado `all_args` ja' montado
/// (`bound_args ++ args_reais`) e o indice do rest param, move `all_args[idx..]`
/// pra um `Entry::Vec` e deixa `all_args = all_args[..idx] ++ [vec_handle]` —
/// o corpo da lambda ve o rest como UM Handle de array. Como o rest e' o ultimo
/// param e as capturas sao prepended, `idx = arity-1` cobre capturas+fixos.
/// Args fixos faltantes (< idx) sao completados com 0. No-op se rest_idx < 0.
fn pack_variadic_tail(all_args: &mut Vec<i64>, rest_idx: i32) {
    if rest_idx < 0 {
        return;
    }
    let ri = rest_idx as usize;
    while all_args.len() < ri {
        all_args.push(0);
    }
    let extra: Vec<i64> = all_args.split_off(ri);
    let vec_h = alloc_entry(Entry::Vec(Box::new(extra)));
    all_args.push(vec_h as i64);
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
pub fn invoke_array_callback(handle_or_ptr: u64, extra: &[i64]) -> i64 {
    if let Some((fn_ptr, bound, _hbt, _bt, _arrow, _htp, param_kinds, ret_kind)) =
        read_function_data(handle_or_ptr)
    {
        let mut all: Vec<i64> = Vec::with_capacity(bound.len() + extra.len());
        all.extend_from_slice(&bound);
        all.extend_from_slice(extra);
        // (cross-runtime closures) extra = [acc/val, idx, arr] vem como inteiro
        // CRU; um callback com param `number` (pk==1) espera bits f64. Reencoda
        // os crus (já-bits passam intactos, pk!=1 idem).
        normalize_f64_bits_args(&mut all, &param_kinds);
        unsafe { invoke_typed(fn_ptr, &all, &param_kinds, ret_kind) }
    } else if let Some((kinds, rk)) = lookup_fn_kinds(handle_or_ptr) {
        // (cross-runtime closures) fn_ptr cru COM ABI registrada (user fn
        // address-taken usada como callback): invoke_typed com os kinds reais +
        // normaliza args number-cru. Cobre `arr.reduce(namedFn)` etc.
        let mut all = extra.to_vec();
        normalize_f64_bits_args(&mut all, &kinds);
        unsafe { invoke_typed(handle_or_ptr, &all, &kinds, rk) }
    } else {
        // fn_ptr cru. Callbacks de array method recebem (val, idx, arr).
        unsafe { invoke_all_i64(handle_or_ptr, extra) }
    }
}

/// Reifica uma user fn estatica (fn_ptr conhecido em compile time) num
/// handle Function. Codegen emite essa call quando ve member access em
/// ident de user fn (ex: `myFn.bind(this)` ou `myFn.name`).
#[rtse::abi("__RTS_FN_GL_FUNCTION_REIFY")]
pub fn __RTS_FN_GL_FUNCTION_REIFY(
    fn_ptr: u64,
    arity: i64,
    name_ptr: i64,
    name_len: i64,
    is_arrow: i32,
    has_this_param: i32,
) -> u64 {
    __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED(
        fn_ptr,
        arity,
        name_ptr,
        name_len,
        is_arrow,
        has_this_param,
        0,
        0,
        0,
        0,
        0,
    )
}

/// REIFY com signature ABI (`param_kinds_ptr/len`, `return_kind`).
/// Usado para reificação de método de classe com tipos não-i64.
/// `param_kinds` codifica cada param: 0=i64, 1=f64, 2=bool, 3=i32 (este
/// codifica o param já incluindo `this` quando aplicável).
#[rtse::abi("__RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED")]
pub fn __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED(
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
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: false,
    })))
}

/// `new Function(...args, body)` — compila body em runtime via eval.
/// Args: `params_handle` (string handle com nomes separados por virgula),
/// `body_handle` (string handle com codigo do body).
#[rtse::abi("__RTS_FN_GL_FUNCTION_NEW")]
pub fn __RTS_FN_GL_FUNCTION_NEW(params_handle: u64, body_handle: u64) -> u64 {
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
        packed_shim: 0,
        source: Some(
            format!("function anonymous({}) {{\n{}\n}}", params_str, body_str).into_boxed_str(),
        ),
        keep_alive: Some(compiled.keep_alive),
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: compiled.uniform,
    })))
}

/// `fn.call(thisArg, argsVec)`. Args vem como Vec handle (codegen empacota).
#[rtse::abi("__RTS_FN_GL_FUNCTION_CALL")]
pub fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64 {
    // (#218 phase2) Proxy callable: redireciona pra trap apply ou forward.
    if let Some((target, handler)) = proxy_resolve(handle) {
        return unsafe { __RTS_FN_RT_PROXY_DISPATCH_APPLY(target, handler, this_arg, args_handle) };
    }
    // dynfn v2: um `new Function` UNIFORME (fn_ptr = thunk de words) chamado pela
    // superfície CRUA — boxa cada arg i64 (raw_to_word), invoca pelo caminho
    // uniforme e decodifica o retorno (word_to_raw) pro slot i64. Sem a ponte,
    // o thunk recebia crus como words e devolvia word cru ao caller.
    if read_uniform_thunk(handle) {
        let raw = read_args_vec(args_handle);
        let words: Vec<i64> = raw.iter().map(|&a| raw_to_word(a)).collect();
        let wh = rts_engine::heap::handles::alloc_entry(
            rts_engine::heap::handles::Entry::Vec(Box::new(words)),
        );
        let r = __RTS_FN_RT_INVOKE_AUTO(handle as i64, this_arg, wh) as u64;
        return word_to_raw(r);
    }
    // (cross-runtime #218/#354) `handle` pode ser um func_addr CRU (arrow/fn
    // passada como VALOR a um param any/function e invocada via .apply/.call —
    // ex. o `target` de um Proxy apply-trap: `apply(t,th,a){ t.apply(th,a) }`).
    // O codegen registra cada user fn address-taken (com >=1 param f64) em
    // FN_KINDS_REGISTRY pelo fn_ptr. Consulta-se ANTES de read_function_data
    // (handle valido nunca colide com um fn_ptr). Sem isto devolvia 0.
    {
        let kinds = {
            let reg = fn_kinds_registry()
                .read()
                .unwrap_or_else(|e| e.into_inner());
            reg.get(&handle).cloned()
        };
        if let Some((param_kinds, return_kind)) = kinds {
            let mut args = read_args_vec(args_handle);
            normalize_f64_bits_args(&mut args, &param_kinds);
            return unsafe { invoke_typed(handle, &args, &param_kinds, return_kind) };
        }
    }
    let (
        fn_ptr,
        bound_args,
        has_bound_this,
        bound_this,
        is_arrow,
        has_this_param,
        param_kinds,
        return_kind,
    ) = match read_function_data(handle) {
        Some(d) => d,
        None => {
            // (cross-runtime #344) `handle` is not a Function entry. If it is
            // a VALID GC handle of some other kind (Map/Vec/...), it is not
            // callable → 0. Otherwise it is a RAW func_addr (a fn-value passed
            // through `any` and invoked via `.call`/`.apply` — e.g. a 0-arg or
            // non-f64-param fn that isn't in FN_KINDS_REGISTRY). Invoke it
            // directly, mirroring INVOKE_AUTO's raw-fn_ptr fallback. Without
            // this, `f.call(null)` on such a param returned 0 instead of
            // invoking f.
            let is_valid_handle = with_entry(handle, |e| e.is_some());
            if is_valid_handle {
                return 0;
            }
            let args = read_args_vec(args_handle);
            rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(this_arg);
            let r = unsafe { invoke_n(handle, &args) };
            rts_engine::heap::this_slot::__RTS_FN_RT_THIS_POP();
            return r;
        }
    };

    let mut all_args = bound_args;
    all_args.extend(read_args_vec(args_handle));
    // (cross-runtime closures) Preenche args faltantes com os defaults da fn
    // antes de empacotar variadic — `fn(...args)` (indireto) que omite um param
    // com default (`acc = 1`) recebia 0. So' p/ não-variadic (param_kinds fixo).
    if read_rest_param_idx(handle) < 0 && !param_kinds.is_empty() {
        pad_with_defaults(&mut all_args, fn_ptr, param_kinds.len());
    }
    // (#195) Variadic: empacota `all_args[rest_idx..]` num array ANTES de
    // tratar `this`/dispatch (lambdas liftadas variadic sao is_arrow → sem
    // insercao de this, entao rest_idx mapeia direto sobre all_args).
    pack_variadic_tail(&mut all_args, read_rest_param_idx(handle));

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
        rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(effective_this);
        true
    } else if has_bound_this {
        // Arrow com `this` capturado em criação (REIFY_BOUND). Empurra ao
        // slot para que THIS_GET() no body leia o valor correto mesmo quando
        // a arrow é chamada fora do escopo original.
        rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(bound_this);
        true
    } else {
        false
    };

    // Args ja' vem encoded corretamente do codegen (numbers como f64 bits
    // via bitcast). Reflect.apply faz conversao int->f64 antes de chamar
    // FUNCTION_CALL. (cross-runtime closures) Mas args vindos de array/captura/
    // spread podem chegar como inteiro cru — reencoda p/ bits nos params f64.
    let mut typed_args = all_args.clone();
    normalize_f64_bits_args(&mut typed_args, &param_kinds);
    // (cross-runtime #799) Variadic fold: callee binario f64 (ex:
    // `Math.max(a,b)`) chamado via `Reflect.apply(Math.max, null, [...n])`
    // com N > 2 args — RTS nao tem variadic na ABI, mas Math.max/min sao
    // semanticamente reduce f64. Quando param_kinds=[1,1] e return=1 e
    // args.len() > 2, foldamos via chamadas sucessivas.
    // (#1281 packed) Se a fn tem shim packed, despacha por ele — aridade
    // arbitraria, sem teto. O shim ja' embute as coercoes f64/i32 por param.
    let packed_shim = read_packed_shim(handle);
    let result = if packed_shim != 0 {
        unsafe { invoke_packed(packed_shim, &typed_args) }
    } else if param_kinds.len() == 2
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
        rts_engine::heap::this_slot::__RTS_FN_RT_THIS_POP();
    }

    result
}

/// `fn.apply(thisArg, argsArray)`. Mesmo dispatch de call.
#[rtse::abi("__RTS_FN_GL_FUNCTION_APPLY")]
pub fn __RTS_FN_GL_FUNCTION_APPLY(handle: u64, this_arg: i64, args_handle: u64) -> i64 {
    __RTS_FN_GL_FUNCTION_CALL(handle, this_arg, args_handle)
}

/// (cross-runtime #799) Reflect.apply/construct usam essa variante que
/// converte args int->f64 bits antes de chamar CALL, baseado em
/// param_kinds do callee. Vec<i64> de fixture (`[10, 20]`) tem numbers
/// como i64 puros — sem essa conversao, invoke_typed os interpretaria
/// como bits denormal f64.
#[rtse::abi("__RTS_FN_GL_FUNCTION_APPLY_TYPED")]
pub fn __RTS_FN_GL_FUNCTION_APPLY_TYPED(
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
                // (cross-runtime #354) heuristica igual a normalize_f64_bits_args:
                // se `v` JA' parece bits f64 (o call site empacotou o number como
                // bits — ex. args de um Proxy `apply` forward), deixa; senao
                // reencoda o int cru -> bits f64. Antes era sempre
                // `f64_to_i64(v as f64)`, que DUPLO-convertia args ja'-bits
                // (f64bits(2.0) tratado como int 4617e15 -> satura i64::MAX).
                let as_f = f64::from_bits(v as u64);
                let looks_bits =
                    as_f == 0.0 || (as_f.is_finite() && as_f.abs() >= 1e-200 && as_f.abs() < 1e200);
                if looks_bits {
                    v
                } else {
                    f64_to_i64(v as f64)
                }
            } else {
                v
            }
        })
        .collect();
    // (cross-runtime #799) Variadic fold: callee binario f64 (Math.max/min)
    // chamado com N > 2 args — JS spec exige varargs; RTS ABI binaria nao
    // tem como expressar diretamente. Foldamos via chamadas sucessivas.
    if param_kinds.len() == 2 && param_kinds == vec![1u8, 1u8] && converted.len() > 2 {
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

/// (cross-runtime closures) Registry `fn_ptr -> (param_kinds, return_kind)`.
/// O codegen reifica VALORES de user fn como `func_addr` cru (não handle) para
/// preservar o `call_indirect` rápido; mas quando essa fn vira uma captura/arg
/// dinâmico e é invocada via INVOKE_AUTO, o caminho raw não conhecia os
/// param_kinds e tratava tudo como i64, perdendo args f64 (`next(25)`→`next(0)`).
/// O codegen agora registra cada user fn address-taken aqui no startup; o
/// fallback raw consulta a tabela e invoca com a ABI correta.
static FN_KINDS_REGISTRY: std::sync::OnceLock<
    std::sync::RwLock<std::collections::HashMap<u64, (Vec<u8>, u8)>>,
> = std::sync::OnceLock::new();

fn fn_kinds_registry() -> &'static std::sync::RwLock<std::collections::HashMap<u64, (Vec<u8>, u8)>>
{
    FN_KINDS_REGISTRY.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()))
}

/// A ABI registrada (param_kinds, return_kind) de uma user fn pelo endereço —
/// `None` quando o codegen não a registrou. Consumido pelos spawners de thread
/// (`thread.spawn` invoca `fn(f64)` quando o worker declara `arg: number`).
pub fn registered_fn_kinds(fn_ptr: u64) -> Option<(Vec<u8>, u8)> {
    fn_kinds_registry()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&fn_ptr)
        .cloned()
}

/// Registra a ABI (param_kinds + return_kind) de uma user fn pelo seu endereço.
/// Emitido pelo codegen no início de `__rts_startup` para cada fn address-taken.
#[rtse::abi("__RTS_FN_RT_REGISTER_FN_KINDS")]
pub fn __RTS_FN_RT_REGISTER_FN_KINDS(
    fn_ptr: u64,
    kinds_ptr: i64,
    kinds_len: i64,
    return_kind: i32,
) {
    if fn_ptr == 0 {
        return;
    }
    let kinds: Vec<u8> = if kinds_ptr != 0 && kinds_len > 0 {
        unsafe { std::slice::from_raw_parts(kinds_ptr as *const u8, kinds_len as usize) }.to_vec()
    } else {
        Vec::new()
    };
    let mut reg = fn_kinds_registry()
        .write()
        .unwrap_or_else(|e| e.into_inner());
    reg.insert(fn_ptr, (kinds, return_kind as u8));
}

/// (cross-runtime closures) Registry `fn_ptr -> default values` (já no encoding
/// do param: bits-f64 p/ number, raw p/ i64/bool). Sentinela `i64::MIN` = sem
/// default. Permite aplicar defaults de params quando a fn é chamada
/// INDIRETAMENTE (`fn(...args)` via handle/INVOKE_AUTO) com menos args — o
/// preenchimento padrão é 0, que ignora `acc = 1` (trampoline/curry → acc=0).
const NO_DEFAULT: i64 = i64::MIN;
static FN_DEFAULTS_REGISTRY: std::sync::OnceLock<
    std::sync::RwLock<std::collections::HashMap<u64, Vec<i64>>>,
> = std::sync::OnceLock::new();

fn fn_defaults_registry() -> &'static std::sync::RwLock<std::collections::HashMap<u64, Vec<i64>>> {
    FN_DEFAULTS_REGISTRY.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()))
}

/// Preenche `args` até `arity` usando os defaults registrados de `fn_ptr`
/// (ou 0 quando não há default). No-op se já tem args suficientes.
fn pad_with_defaults(args: &mut Vec<i64>, fn_ptr: u64, arity: usize) {
    if args.len() >= arity {
        return;
    }
    let defs = {
        let reg = fn_defaults_registry()
            .read()
            .unwrap_or_else(|e| e.into_inner());
        reg.get(&fn_ptr).cloned()
    };
    while args.len() < arity {
        let i = args.len();
        let v = defs
            .as_ref()
            .and_then(|d| d.get(i).copied())
            .filter(|&v| v != NO_DEFAULT)
            .unwrap_or(0);
        args.push(v);
    }
}

/// Lookup da ABI registrada de um `fn_ptr` cru. `None` quando não registrado.
fn lookup_fn_kinds(fn_ptr: u64) -> Option<(Vec<u8>, u8)> {
    let reg = fn_kinds_registry()
        .read()
        .unwrap_or_else(|e| e.into_inner());
    reg.get(&fn_ptr).cloned()
}

/// (cross-runtime closures) Invoca um `fn_ptr` CRU de user fn com `args`,
/// consultando o registry de ABI: se registrado (param f64), normaliza os args
/// number-cru → bits e usa invoke_typed com a ABI correta; senão cai no
/// invoke_n (todos i64). Usado por callers que têm só o ponteiro resolvido (ex:
/// promise.then), p/ não chamar uma fn `(f64)->f64` via ABI i64 (segfault).
pub fn invoke_fn_ptr_with_registry(fn_ptr: u64, args: &[i64]) -> i64 {
    // (cross-runtime closures) Se na verdade for um HANDLE Function (escapou sem
    // resolve — ex: callback de `.then` reificado como handle quando passou a
    // ter param `number`/f64), despacha pelo caminho de handle: prepend bound,
    // normaliza, invoke_typed com os kinds do handle. Sem isto o invoke_n abaixo
    // saltava para o valor do handle (0x1000...) → ACCESS_VIOLATION.
    if let Some((real_ptr, bound, _hbt, _bt, _arrow, _htp, kinds, rk)) = read_function_data(fn_ptr)
    {
        let mut a = bound;
        a.extend_from_slice(args);
        normalize_f64_bits_args(&mut a, &kinds);
        return unsafe { invoke_typed(real_ptr, &a, &kinds, rk) };
    }
    if let Some((kinds, rk)) = lookup_fn_kinds(fn_ptr) {
        let mut a = args.to_vec();
        // (cross-runtime closures) Preenche params omitidos com os defaults
        // registrados ANTES de normalizar — `fn(...args)` indireto a uma fn com
        // `acc = 1` recebia acc=0 (trampoline/curry com fn-expr inline raw).
        pad_with_defaults(&mut a, fn_ptr, kinds.len());
        normalize_f64_bits_args(&mut a, &kinds);
        unsafe { invoke_typed(fn_ptr, &a, &kinds, rk) }
    } else {
        unsafe { invoke_n(fn_ptr, args) }
    }
}

/// (cross-runtime #336) Object.prototype singleton — Map cacheado com
/// `constructor: {name: "Object"}`. Usado em prototype chain inspection:
/// quando uma classe raiz (sem super) e' criada, seu proto Map recebe
/// `__proto__ = OBJECT_PROTOTYPE_HANDLE()`, garantindo que iteracao
/// chain `while (proto) { ... proto = Object.getPrototypeOf(proto); }`
/// termine com `proto.constructor.name === "Object"` antes do null.
///
/// INTENCIONALMENTE separado do `object_proto_root` do engine
/// (`rts-runtime::adapters::value::protos`, um objeto Vec-keyed com slots reais): este e'
/// Map-backed e serve a chain legacy de Map/Set (string-sentinels), numa CAMADA
/// que NAO pode alcancar o rts-runtime (rts-shared/rts-primitives estao ABAIXO
/// dele). Reps incompativeis (Map vs Vec-keyed) + layering ⇒ NAO e' uma duplicata
/// de proto do engine a unificar; e' um artefato de camada aceito.
#[rtse::abi("__RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE")]
pub fn __RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE() -> u64 {
    static SINGLETON: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *SINGLETON.get_or_init(|| {
        let proto = unsafe { __RTS_FN_NS_COLLECTIONS_MAP_NEW() };
        let ctor_stub = alloc_entry(Entry::Function(Box::new(
            rts_engine::heap::handles::FunctionData {
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
                packed_shim: 0,
                source: None,
                keep_alive: None,
                prototype_handle: 0,
                rest_param_idx: -1,
                uniform_thunk: false,
            },
        )));
        unsafe {
            __RTS_FN_RT_MAP_SET_STR(proto, b"constructor".as_ptr(), 11, ctor_stub as i64);
            // (cross-runtime #377) constructor eh non-enumerable em Object.prototype.
            __RTS_FN_RT_MAP_MARK_NON_ENUM(proto, b"constructor".as_ptr(), 11);
        }
        proto
    })
}

/// (#proto-method) Auto-dispatch: se \`callee\` eh handle Function valido,
/// chama via invoke_typed (com return_kind correto). Senao trata como
/// fn_ptr cru e faz invoke_n (todos i64). Usado por
/// \`lower_var_member_call\` quando member access em handle pode resolver
/// para fn ptr OU para handle Function (depende se foi REIFY-ed).
///
/// `this_arg` so eh empilhado quando callee eh Function handle (caminho
/// classes). Args vem de Vec<i64> handle.
#[rtse::abi("__RTS_FN_RT_INVOKE_AUTO")]
pub fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64 {
    invoke_auto_impl(callee, this_arg, args_handle, None)
}

/// Invoca um CALLABLE em qualquer das convenções vivas, com args crus:
/// - uma WORD `TAG_FUNCTION` do motor novo → normaliza para o handle real;
/// - um handle `Entry::Function` vivo → `INVOKE_AUTO` (aplica bound_args,
///   param_kinds, env de uniform-thunk — a convenção correta do fn VALUE);
/// - um fn_ptr cru legado → `invoke_fn_ptr_with_registry` (kinds do registry
///   quando conhecidos, senão o transmute i64 histórico).
///
/// É a ponte ÚNICA usada pelos consumidores de callback do runtime (watcher de
/// `promise.create`, drain de microtasks then/catch/finally) — sem ela, um
/// callback uniform-thunk era invocado como fn crua e lia o arg no slot do env
/// (valores "a0b0" no lugar de "a1b2").
pub fn invoke_callable_auto(callable: u64, args: &[i64]) -> i64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
    let h = rts_engine::heap::poly::poly_handle_normalize(callable).unwrap_or(callable);
    let is_fn_handle = with_entry(h, |e| matches!(e, Some(Entry::Function(_))));
    if is_fn_handle {
        let args_h = alloc_entry(Entry::Vec(Box::new(args.to_vec())));
        return __RTS_FN_RT_INVOKE_AUTO(h as i64, 0, args_h);
    }
    invoke_fn_ptr_with_registry(callable, args)
}

/// Ponte de CONVENÇÃO COMPLETA para callbacks da superfície i64 (`promise.then`
/// / `promise.create` / drain de microtasks): um callee UNIFORM-THUNK (fn VALUE
/// do motor novo) fala PolyValue WORDS nos dois sentidos; a superfície fala i64
/// CRU. `args_are_words` marca a proveniência dos args (`true` = array do motor
/// novo, já words; `false` = valor settled cru da superfície). O retorno volta
/// SEMPRE cru — INT32 → valor, singleton → 0, heap-tag → handle real, double
/// inline → trunc — para o slot i64 (o `await` do motor novo re-boxa números).
/// Um callee NÃO-uniform (fn_ptr cru / `new Function` legado) recebe args CRUS
/// (words decodificadas quando `args_are_words`) e devolve verbatim.
pub fn invoke_callable_bridged(callable: u64, args: &[i64], args_are_words: bool) -> i64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
    let h = rts_engine::heap::poly::poly_handle_normalize(callable).unwrap_or(callable);
    let uniform = with_entry(h, |e| {
        matches!(e, Some(Entry::Function(d)) if d.uniform_thunk)
    });
    if uniform {
        let words: Vec<i64> = if args_are_words {
            args.to_vec()
        } else {
            args.iter().map(|&a| raw_to_word(a)).collect()
        };
        let args_h = alloc_entry(Entry::Vec(Box::new(words)));
        let r = __RTS_FN_RT_INVOKE_AUTO(h as i64, 0, args_h) as u64;
        return word_to_raw(r);
    }
    let raw: Vec<i64> = if args_are_words {
        args.iter().map(|&a| word_to_raw(a as u64)).collect()
    } else {
        args.to_vec()
    };
    invoke_callable_auto(callable, &raw)
}

/// i64 CRU da superfície → WORD PolyValue numérica (INT32 quando cabe, senão os
/// bits do f64). Um valor que JÁ está no quadrante boxed passa verbatim (o
/// produtor já falava words).
fn raw_to_word(a: i64) -> i64 {
    use rts_engine::heap::poly as p;
    let w = a as u64;
    if (w & p::POLY_BOX_BASE) == p::POLY_BOX_BASE {
        return a;
    }
    if let Ok(i) = i32::try_from(a) {
        // TAG_INT32 = 1 (the PolyValue small-int tag).
        return (p::POLY_BOX_BASE | (1u64 << p::POLY_TAG_SHIFT) | (i as u32 as u64)) as i64;
    }
    // Um HANDLE GC cru (generation bits ⇒ ≥ 2^48, entry viva — e.g. o settled
    // de `Promise.reject("error")` chegando num `.catch`): boxa por entry kind
    // (STR/OBJECT) pro callee uniforme ler o valor real, não um número gigante.
    if w >= (1u64 << 48) {
        use rts_engine::heap::handles::{Entry, with_entry};
        let tag: u64 = with_entry(w, |e| match e {
            Some(Entry::String(_)) => p::POLY_TAG_STR,
            Some(Entry::Function(_)) => p::POLY_TAG_FUNCTION,
            Some(_) => p::POLY_TAG_OBJECT,
            None => 0,
        });
        if tag != 0 {
            let slot = rts_engine::heap::handles::__rtsn_poly_from_handle(w);
            return (p::POLY_BOX_BASE | (tag << p::POLY_TAG_SHIFT) | slot) as i64;
        }
    }
    f64::to_bits(a as f64) as i64
}

/// WORD PolyValue → i64 CRU da superfície: INT32 → valor, singleton → 0,
/// heap-tag (STR/OBJECT/FUNCTION) → handle real, double inline → trunc. Um i64
/// pequeno não-boxed (já cru — bits abaixo de 2^48, onde nenhum double normal
/// vive) passa verbatim.
fn word_to_raw(w: u64) -> i64 {
    use rts_engine::heap::poly as p;
    if (w & p::POLY_BOX_BASE) == p::POLY_BOX_BASE {
        let tag = (w >> p::POLY_TAG_SHIFT) & p::POLY_TAG_MASK;
        return match tag {
            1 => (w & 0xFFFF_FFFF) as u32 as i32 as i64,
            p::POLY_TAG_SINGLETON => 0,
            p::POLY_TAG_STR | p::POLY_TAG_OBJECT | p::POLY_TAG_FUNCTION => {
                p::poly_handle_normalize(w).unwrap_or(0) as i64
            }
            _ => w as i64,
        };
    }
    if w > (1u64 << 48) {
        f64::from_bits(w) as i64
    } else {
        w as i64
    }
}

fn invoke_auto_impl(
    callee: i64,
    this_arg: i64,
    args_handle: u64,
    override_return_kind: Option<u8>,
) -> i64 {
    // (#218 phase2) Proxy callable: se callee for Entry::Proxy, despacha
    // pra trap `apply` ou faz forward chamando o target via Function.apply.
    if let Some((target, handler)) = proxy_resolve(callee as u64) {
        return unsafe { __RTS_FN_RT_PROXY_DISPATCH_APPLY(target, handler, this_arg, args_handle) };
    }
    // Motor NOVO: `fn_ptr` é um THUNK de ABI UNIFORME (env, a0..a3, rest) — 6
    // slots PolyValue, env em bound_this. A convenção legada por aridade abaixo
    // invocaria com os slots errados (env perdido → capturas quebradas). Args
    // além de 4 ficam de fora (rest=0) — os pumps de timer/microtask chamam com
    // 0 args, o caso observado.
    if read_uniform_thunk(callee as u64) {
        if let Some((fn_ptr, _bound, _hbt, bound_this, ..)) = read_function_data(callee as u64) {
            if fn_ptr != 0 {
                let args = read_args_vec(args_handle);
                // Absent slots ride as the UNDEFINED word (not 0): the uniform
                // thunk's `...rest` pack trims trailing undefineds — a raw 0
                // would be packed as a bogus element.
                let undef = rts_engine::heap::poly::POLY_UNDEFINED;
                let a = |i: usize| args.get(i).map(|&v| v as u64).unwrap_or(undef);
                type Uniform = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;
                let f: Uniform = unsafe { std::mem::transmute(fn_ptr as usize) };
                return unsafe { f(bound_this as u64, a(0), a(1), a(2), a(3), undef) } as i64;
            }
        }
        return 0;
    }
    // Tenta como handle Function primeiro.
    if let Some((
        fn_ptr,
        bound_args,
        has_bound_this,
        bound_this,
        is_arrow,
        has_this_param,
        param_kinds,
        return_kind,
    )) = read_function_data(callee as u64)
    {
        let mut all_args = bound_args;
        all_args.extend(read_args_vec(args_handle));
        // (#195) Variadic: empacota o tail num array handle antes do pad/dispatch.
        pack_variadic_tail(&mut all_args, read_rest_param_idx(callee as u64));
        // (cross-runtime closures) Preenche args faltantes com os DEFAULTS
        // registrados da fn (ou 0 quando não há) até atender param_kinds. Antes
        // era sempre 0, ignorando `acc = 1` em call indireto (`fn(...args)` de
        // trampoline/curry → acc=0). pad_with_defaults usa o registry por fn_ptr.
        pad_with_defaults(&mut all_args, fn_ptr, param_kinds.len());
        let pushed_this = if !is_arrow && has_this_param {
            let effective = if has_bound_this { bound_this } else { this_arg };
            all_args.insert(0, effective);
            false
        } else if !is_arrow {
            let effective = if has_bound_this { bound_this } else { this_arg };
            rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(effective);
            true
        } else if has_bound_this {
            // Arrow com `this` capturado em criação (REIFY_BOUND). Empurra ao
            // slot para que THIS_GET() no body leia o valor correto mesmo quando
            // a arrow é chamada fora do escopo original.
            rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(bound_this);
            true
        } else {
            false
        };
        // Override do call site so' quando o handle nao declara return_kind.
        let effective_rk = match override_return_kind {
            Some(ov) if return_kind == 0 => ov,
            _ => return_kind,
        };
        // (cross-runtime closures) Reencoda args number-cru → bits f64 quando o
        // param é f64 (caminho INVOKE_AUTO/captura/spread que empacota i64 cru).
        normalize_f64_bits_args(&mut all_args, &param_kinds);
        // (#1281 packed) Despacha pelo shim quando presente (aridade arbitraria).
        let packed_shim = read_packed_shim(callee as u64);
        let r = if packed_shim != 0 {
            unsafe { invoke_packed(packed_shim, &all_args) }
        } else {
            unsafe { invoke_typed(fn_ptr, &all_args, &param_kinds, effective_rk) }
        };
        if pushed_this {
            rts_engine::heap::this_slot::__RTS_FN_RT_THIS_POP();
        }
        return r;
    }
    // Fn ptr raw — usa invoke_typed com override (param_kinds vazio).
    let mut args_v = read_args_vec(args_handle);
    rts_engine::heap::this_slot::__RTS_FN_RT_THIS_PUSH(this_arg);
    // (cross-runtime closures) Se o codegen registrou a ABI desse fn_ptr cru
    // (user fn address-taken capturada/passada como valor), invoca com os
    // param_kinds/return_kind reais — normalizando args number-cru p/ bits f64.
    // Sem isto `next(25)` numa closure virava `next(0)` (f64 arg perdido).
    let r = if let Some((kinds, reg_rk)) = lookup_fn_kinds(callee as u64) {
        pad_with_defaults(&mut args_v, callee as u64, kinds.len());
        normalize_f64_bits_args(&mut args_v, &kinds);
        let rk = match override_return_kind {
            Some(ov) if reg_rk == 0 => ov,
            _ => reg_rk,
        };
        unsafe { invoke_typed(callee as u64, &args_v, &kinds, rk) }
    } else {
        let rk = override_return_kind.unwrap_or(0);
        if rk == 0 {
            unsafe { invoke_n(callee as u64, &args_v) }
        } else {
            unsafe { invoke_typed(callee as u64, &args_v, &[], rk) }
        }
    };
    rts_engine::heap::this_slot::__RTS_FN_RT_THIS_POP();
    r
}

#[rtse::abi("__RTS_FN_GL_FUNCTION_NAME")]
pub fn __RTS_FN_GL_FUNCTION_NAME(handle: u64) -> u64 {
    let name = with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            d.name.to_string()
        } else {
            String::new()
        }
    });
    alloc_entry(Entry::String(name.into_bytes()))
}

#[rtse::abi("__RTS_FN_GL_FUNCTION_LENGTH")]
pub fn __RTS_FN_GL_FUNCTION_LENGTH(handle: u64) -> i64 {
    with_entry(handle, |e| {
        if let Some(Entry::Function(d)) = e {
            (d.arity as i64)
                .saturating_sub(d.bound_args.len() as i64)
                .max(0)
        } else {
            0
        }
    })
}
