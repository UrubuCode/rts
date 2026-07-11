//! `node:buffer` — the static / global surface that works without the
//! instance-method wall: `atob`/`btoa` and the `Buffer` STATIC methods
//! (`alloc`/`allocUnsafe`/`from`/`isBuffer`/`byteLength`/`concat`/`compare`),
//! which dispatch statically and return `Uint8Array`-shaped arrays. Real bytes.
//!
//! Deferred (blocked on runtime object-backed-class dispatch — a Buffer read
//! from a variable/array can't dispatch instance methods): the Buffer INSTANCE
//! methods (`toString`/`write`/`slice`/`readUInt8`/`writeUInt8`/`equals`/`fill`/
//! `indexOf`/`copy`/…), and the `Blob`/`File` classes (need blob/stream backing).
//!
//! Layout: `ops` (base64 + byte ops + extern points), `mod` (registration).

mod ops;

use rts_engine::AbiType::{self, Bool, Handle, I64, StrPtr};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn member(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `Buffer` class (statics) + the `node:buffer` module.
pub fn register(e: &mut Engine) {
    use ops as s;
    use MemberKind::{Function, StaticMethod};

    e.class("Buffer")
        .doc("Buffer — byte container (node:buffer). Static constructors + helpers.")
        .member(member("alloc", StaticMethod, vec![I64], Handle, "__RTS_FN_NODE_BUFFER_ALLOC", "alloc(size: number): number[]", s::__RTS_FN_NODE_BUFFER_ALLOC as *const u8))
        .member(member("allocUnsafe", StaticMethod, vec![I64], Handle, "__RTS_FN_NODE_BUFFER_ALLOC", "allocUnsafe(size: number): number[]", s::__RTS_FN_NODE_BUFFER_ALLOC as *const u8))
        .member(member("from", StaticMethod, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_FROM", "from(string: string): number[]", s::__RTS_FN_NODE_BUFFER_FROM as *const u8))
        .member(member("from", StaticMethod, vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_BUFFER_FROM_ENC", "from(string: string, encoding: string): number[]", s::__RTS_FN_NODE_BUFFER_FROM_ENC as *const u8))
        .member(member("isBuffer", StaticMethod, vec![Handle], Bool, "__RTS_FN_NODE_BUFFER_IS_BUFFER", "isBuffer(obj: object): boolean", s::__RTS_FN_NODE_BUFFER_IS_BUFFER as *const u8))
        .member(member("byteLength", StaticMethod, vec![StrPtr], I64, "__RTS_FN_NODE_BUFFER_BYTE_LENGTH", "byteLength(string: string): number", s::__RTS_FN_NODE_BUFFER_BYTE_LENGTH as *const u8))
        .member(member("compare", StaticMethod, vec![Handle, Handle], I64, "__RTS_FN_NODE_BUFFER_COMPARE", "compare(a: object, b: object): number", s::__RTS_FN_NODE_BUFFER_COMPARE as *const u8))
        .member(member("concat", StaticMethod, vec![Handle], Handle, "__RTS_FN_NODE_BUFFER_CONCAT", "concat(list: object): number[]", s::__RTS_FN_NODE_BUFFER_CONCAT as *const u8))
        .done();

    e.ns("node:buffer")
        .doc("Buffer/base64 (node:buffer): atob, btoa.")
        .member(member("atob", Function, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_ATOB", "atob(data: string): string", s::__RTS_FN_NODE_BUFFER_ATOB as *const u8))
        .member(member("btoa", Function, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_BTOA", "btoa(data: string): string", s::__RTS_FN_NODE_BUFFER_BTOA as *const u8))
        .done();
}
