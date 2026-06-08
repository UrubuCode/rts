//! Error class family (Error/TypeError/RangeError/ReferenceError/SyntaxError/
//! URIError/EvalError/AggregateError). Migrado ao modelo `#[rts_class]` (stage 5)
//! via membros `external` — os externs `__RTS_FN_GL_*ERROR*` ficam em
//! `instance.rs`/`rt.rs` intactos; o macro deriva apenas os 8 `*_CLASS_SPEC`.
//! Os membros message/name/toString/cause compartilham os mesmos externs
//! `__RTS_FN_GL_ERROR_*` entre todas as classes (via `symbol =` explicito).

pub mod instance;

// All members are `external` (externs live in instance.rs/rt.rs); the type
// tokens live only in the macro-consumed stubs.
#[allow(unused_imports)]
use rts_abi::ty::Handle;
use rts_macro::rts_class;

/// Error.
#[rts_class(Error, spec = "CLASS_SPEC")]
impl ErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_ERROR_NEW",
        ts = "new Error(message?: string, options?: object): Error"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_smethod(
        external,
        name = "captureStackTrace",
        symbol = "__RTS_FN_GL_ERROR_CAPTURE_STACK_TRACE",
        ts = "captureStackTrace(target: object, ctor?: Function): void"
    )]
    pub fn capture_stack_trace(_target: Handle, _ctor: Handle) {
        unreachable!()
    }
}

/// TypeError.
#[rts_class(TypeError, spec = "TYPE_ERROR_CLASS_SPEC")]
impl TypeErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_TYPE_ERROR_NEW",
        ts = "new TypeError(message?: string, options?: object): TypeError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// RangeError.
#[rts_class(RangeError, spec = "RANGE_ERROR_CLASS_SPEC")]
impl RangeErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_RANGE_ERROR_NEW",
        ts = "new RangeError(message?: string, options?: object): RangeError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// ReferenceError.
#[rts_class(ReferenceError, spec = "REF_ERROR_CLASS_SPEC")]
impl ReferenceErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_REF_ERROR_NEW",
        ts = "new ReferenceError(message?: string, options?: object): ReferenceError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// SyntaxError.
#[rts_class(SyntaxError, spec = "SYNTAX_ERROR_CLASS_SPEC")]
impl SyntaxErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_SYNTAX_ERROR_NEW",
        ts = "new SyntaxError(message?: string, options?: object): SyntaxError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// URIError.
#[rts_class(URIError, spec = "URI_ERROR_CLASS_SPEC")]
impl UriErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_URI_ERROR_NEW",
        ts = "new URIError(message?: string, options?: object): URIError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// EvalError.
#[rts_class(EvalError, spec = "EVAL_ERROR_CLASS_SPEC")]
impl EvalErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_EVAL_ERROR_NEW",
        ts = "new EvalError(message?: string, options?: object): EvalError"
    )]
    pub fn new(_message: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "cause",
        symbol = "__RTS_FN_GL_ERROR_CAUSE",
        ts = "cause: any"
    )]
    pub fn cause(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// AggregateError — ctor(errors, message); tem `.errors`, sem `.cause`.
#[rts_class(AggregateError, spec = "AGGREGATE_ERROR_CLASS_SPEC")]
impl AggregateErrorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_AGGREGATE_ERROR_NEW",
        ts = "new AggregateError(errors: any[], message?: string): AggregateError"
    )]
    pub fn new(_errors: Handle, _message: Str) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "message",
        symbol = "__RTS_FN_GL_ERROR_MESSAGE",
        ts = "message: string"
    )]
    pub fn message(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "name",
        symbol = "__RTS_FN_GL_ERROR_NAME",
        ts = "name: string"
    )]
    pub fn err_name(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "toString",
        symbol = "__RTS_FN_GL_ERROR_TO_STRING",
        ts = "toString(): string"
    )]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "errors",
        symbol = "__RTS_FN_GL_AGGREGATE_ERROR_ERRORS",
        ts = "errors: any[]"
    )]
    pub fn errors(_h: Handle) -> Handle {
        unreachable!()
    }
}
