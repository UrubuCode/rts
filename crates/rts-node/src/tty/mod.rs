//! `node:tty` — TTY detection. Registration only; see `symbols.rs` for the
//! extern "C" implementations.

mod symbols;
mod promises;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:tty` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("node:tty")
        .alias("tty")
        .doc("TTY detection (node:tty).")
        .member(pure_func(
            "isatty",
            "__RTS_FN_NODE_TTY_ISATTY",
            sig!(I64 => Bool),
            "isatty(fd: number): boolean",
            "True when fd (0/1/2 => stdin/stdout/stderr) refers to a TTY.",
            symbols::__RTS_FN_NODE_TTY_ISATTY as *const u8,
        ))
        .done();
}
