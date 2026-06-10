//! Function — GlobalClassSpec para o tipo primitivo JS Function (#359).
//!
//! Cobre `.call`, `.apply`, `.bind`, `.name`, `.length` e o constructor
//! `new Function("a", "b", "return a+b")` via runtime.eval.
//!
//! Limitacoes vs Node:
//! - `.toString()` retorna `"function <name>() { [native code] }"` (RTS nao
//!   preserva source de fns declaradas estaticamente, exceto as criadas
//!   via `new Function`).
//! - `.prototype` nao existe (RTS separa classes de functions).
//! - `arguments` object nao existe (use rest params).
//! - `this` em fn declarations nao-arrow chamadas via `.call(thisArg)`:
//!   thisArg eh ignorado se a fn original nao for method de classe (RTS
//!   fns nao tem slot reservado pra this implicito).

pub mod ops;
pub mod props;

// All members are `external`: the macro consumes the impl and emits only the
// spec (no externs), so the `Handle`/`I64` tokens live only inside the consumed
// stub signatures — the import reads as unused to rustc.
#[allow(unused_imports)]
use rts_engine::abi::ty::{Handle, I64};
use rts_macro::rts_class;

/// Built-in Function class (#359). Todos os membros sao `external` — os externs
/// `__RTS_FN_GL_FUNCTION_*` vivem em `ops.rs`/`props.rs`; aqui o macro só deriva
/// o `FUNCTION_CLASS_SPEC` (stage 5, substitui o antigo `abi.rs`).
#[rts_class(Function)]
impl FunctionClass {
    /// new Function(...args) — via runtime.eval.
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_FUNCTION_NEW",
        ts = "constructor(...args: string[]): Function"
    )]
    pub fn new(_params: Handle, _body: Handle) -> Handle {
        unreachable!()
    }

    /// fn.call(thisArg, ...args)
    #[rts_method(
        external,
        name = "call",
        symbol = "__RTS_FN_GL_FUNCTION_CALL",
        ts = "call(thisArg: any, ...args: any[]): any"
    )]
    pub fn call(_h: Handle, _this_arg: I64, _args: Handle) -> I64 {
        unreachable!()
    }

    /// fn.apply(thisArg, args)
    #[rts_method(
        external,
        name = "apply",
        symbol = "__RTS_FN_GL_FUNCTION_APPLY",
        ts = "apply(thisArg: any, args: any[]): any"
    )]
    pub fn apply(_h: Handle, _this_arg: I64, _args: Handle) -> I64 {
        unreachable!()
    }

    /// fn.bind(thisArg, ...args)
    #[rts_method(
        external,
        name = "bind",
        symbol = "__RTS_FN_GL_FUNCTION_BIND",
        ts = "bind(thisArg: any, ...args: any[]): Function"
    )]
    pub fn bind(_h: Handle, _this_arg: I64, _args: Handle) -> Handle {
        unreachable!()
    }

    /// fn.toString()
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_FUNCTION_TO_STRING",
        ts = "toString(): string",
        pure
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }

    /// fn.name
    #[rts_getter(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_FUNCTION_NAME",
        ts = "readonly name: string",
        pure
    )]
    pub fn name(_h: Handle) -> Handle {
        unreachable!()
    }

    /// fn.length
    #[rts_getter(
        external,
        name = "length",
        symbol = "__RTS_FN_GL_FUNCTION_LENGTH",
        ts = "readonly length: number",
        pure
    )]
    pub fn length(_h: Handle) -> I64 {
        unreachable!()
    }

    /// fn.prototype
    #[rts_getter(
        external,
        name = "prototype",
        symbol = "__RTS_FN_GL_FUNCTION_PROTOTYPE_GET",
        ts = "prototype: any"
    )]
    pub fn prototype(_h: Handle) -> Handle {
        unreachable!()
    }
}
