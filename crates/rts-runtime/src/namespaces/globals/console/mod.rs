//! `console` global namespace — variadic print methods. Migrado ao modelo
//! `#[rts_namespace]` (stage 2c) via membros `external`: os símbolos apontam
//! para `io.*` (codegen concatena os args variádicos antes de chamar). O
//! runtime override side-table (`rt.rs`) fica intacto.
//!
//! Codegen ainda especializa `console.*` porque os métodos são variádicos
//! (nº arbitrário de args de qualquer tipo), o que não cabe no ABI fixo
//! `AbiType[]`; os `symbol` apontam para os alvos `io.*` reais.

pub mod rt;

#[allow(unused_imports)]
use rts_engine::abi::ty::Str;
use rts_macro::rts_namespace;

/// Global console object — variadic print to stdout/stderr.
#[rts_namespace(console, sym = "NS_IO")]
impl ConsoleNs {
    /// Prints args separated by spaces to stdout.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_PRINT",
        ts = "log(...args: unknown[]): void"
    )]
    pub fn log(_args: Str) {
        unreachable!()
    }

    /// Alias for console.log.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_PRINT",
        ts = "info(...args: unknown[]): void"
    )]
    pub fn info(_args: Str) {
        unreachable!()
    }

    /// Alias for console.log.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_PRINT",
        ts = "debug(...args: unknown[]): void"
    )]
    pub fn debug(_args: Str) {
        unreachable!()
    }

    /// Prints args separated by spaces to stderr.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_EPRINT",
        ts = "error(...args: unknown[]): void"
    )]
    pub fn error(_args: Str) {
        unreachable!()
    }

    /// Alias for console.error.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_EPRINT",
        ts = "warn(...args: unknown[]): void"
    )]
    pub fn warn(_args: Str) {
        unreachable!()
    }

    /// If cond is falsy, prints "Assertion failed: <msg>" to stderr; otherwise no-op. Does not throw.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_EPRINT",
        ts = "assert(cond: unknown, ...msg: unknown[]): void"
    )]
    pub fn assert(_args: Str) {
        unreachable!()
    }

    /// Pretty-prints a single argument via INSPECT (alias of console.log for one arg).
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_IO_PRINT",
        ts = "dir(arg: unknown): void"
    )]
    pub fn dir(_args: Str) {
        unreachable!()
    }
}
