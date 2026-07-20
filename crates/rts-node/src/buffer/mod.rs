//! `node:buffer` — the static / global surface that works without the
//! instance-method wall: `atob`/`btoa` and the `Buffer` STATIC methods
//! (`alloc`/`allocUnsafe`/`from`/`isBuffer`/`byteLength`/`concat`/`compare`/
//! `toString`), which dispatch statically and return `Uint8Array`-shaped
//! arrays. Real bytes.
//!
//! `toString`/`toString(encoding)` are exposed as `Buffer.toString(buf[, enc])`
//! — a STATIC call, not `buf.toString()` — because the engine has no proven
//! class tag for a Buffer's `Entry::Vec` receiver to dispatch a real instance
//! method on (a plain array receiver's `.toString()` resolves to
//! `Array.prototype.toString` — comma-joined — long before it could reach a
//! Buffer-specific override). Real `buf.toString()` needs `JsKind`-level
//! Buffer tracking in the front-end's Lowerer (a `resolve_method`
//! `RecvClass::Buffer` row analogous to `RecvClass::Array`'s `ARRAY_ROWS`) —
//! deferred as its own follow-up, not attempted here.
//!
//! Deferred (blocked on the same runtime object-backed-class dispatch gap):
//! the remaining Buffer INSTANCE methods (`write`/`slice`/`readUInt8`/
//! `writeUInt8`/`equals`/`fill`/`indexOf`/`copy`/…), and the `Blob`/`File`
//! classes (need blob/stream backing).
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
        emit: None,
    }
}

/// Flag a member `THROWS` so its RangeError routes to an enclosing try/catch.
fn throwing(mut m: Member) -> Member {
    m.flags = MemberFlags::THROWS;
    m
}

/// Registers the `Buffer` class (statics) + the `node:buffer` module.
pub fn register(e: &mut Engine) {
    use ops as s;
    use MemberKind::{Function, StaticMethod};

    e.class("Buffer")
        .doc("Buffer — byte container (node:buffer). Static constructors + helpers.")
        .member(throwing(member("alloc", StaticMethod, vec![I64], Handle, "__RTS_FN_NODE_BUFFER_ALLOC", "alloc(size: number): number[]", s::__RTS_FN_NODE_BUFFER_ALLOC as *const u8)))
        .member(throwing(member("alloc", StaticMethod, vec![I64, I64], Handle, "__RTS_FN_NODE_BUFFER_ALLOC_FILL", "alloc(size: number, fill: number): number[]", s::__RTS_FN_NODE_BUFFER_ALLOC_FILL as *const u8)))
        .member(throwing(member("allocUnsafe", StaticMethod, vec![I64], Handle, "__RTS_FN_NODE_BUFFER_ALLOC", "allocUnsafe(size: number): number[]", s::__RTS_FN_NODE_BUFFER_ALLOC as *const u8)))
        .member(member("from", StaticMethod, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_FROM", "from(string: string): number[]", s::__RTS_FN_NODE_BUFFER_FROM as *const u8))
        .member(member("from", StaticMethod, vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_BUFFER_FROM_ENC", "from(string: string, encoding: string): number[]", s::__RTS_FN_NODE_BUFFER_FROM_ENC as *const u8))
        .member(member("from", StaticMethod, vec![Handle], Handle, "__RTS_FN_NODE_BUFFER_FROM_ARRAY", "from(data: object): number[]", s::__RTS_FN_NODE_BUFFER_FROM_ARRAY as *const u8))
        .member(member("isBuffer", StaticMethod, vec![Handle], Bool, "__RTS_FN_NODE_BUFFER_IS_BUFFER", "isBuffer(obj: object): boolean", s::__RTS_FN_NODE_BUFFER_IS_BUFFER as *const u8))
        .member(member("isEncoding", StaticMethod, vec![StrPtr], Bool, "__RTS_FN_NODE_BUFFER_IS_ENCODING", "isEncoding(encoding: string): boolean", s::__RTS_FN_NODE_BUFFER_IS_ENCODING as *const u8))
        .member(member("byteLength", StaticMethod, vec![StrPtr], I64, "__RTS_FN_NODE_BUFFER_BYTE_LENGTH", "byteLength(string: string): number", s::__RTS_FN_NODE_BUFFER_BYTE_LENGTH as *const u8))
        .member(member("byteLength", StaticMethod, vec![StrPtr, StrPtr], I64, "__RTS_FN_NODE_BUFFER_BYTE_LENGTH_ENC", "byteLength(string: string, encoding: string): number", s::__RTS_FN_NODE_BUFFER_BYTE_LENGTH_ENC as *const u8))
        .member(member("compare", StaticMethod, vec![Handle, Handle], I64, "__RTS_FN_NODE_BUFFER_COMPARE", "compare(a: object, b: object): number", s::__RTS_FN_NODE_BUFFER_COMPARE as *const u8))
        .member(member("concat", StaticMethod, vec![Handle], Handle, "__RTS_FN_NODE_BUFFER_CONCAT", "concat(list: object): number[]", s::__RTS_FN_NODE_BUFFER_CONCAT as *const u8))
        .member(member("concat", StaticMethod, vec![Handle, I64], Handle, "__RTS_FN_NODE_BUFFER_CONCAT_LEN", "concat(list: object, totalLength: number): number[]", s::__RTS_FN_NODE_BUFFER_CONCAT_LEN as *const u8))
        .member(member("toString", StaticMethod, vec![Handle], Handle, "__RTS_FN_NODE_BUFFER_TO_STRING", "toString(buf: object): string", s::__RTS_FN_NODE_BUFFER_TO_STRING as *const u8))
        .member(member("toString", StaticMethod, vec![Handle, StrPtr], Handle, "__RTS_FN_NODE_BUFFER_TO_STRING_ENC", "toString(buf: object, encoding: string): string", s::__RTS_FN_NODE_BUFFER_TO_STRING_ENC as *const u8))
        .done();

    e.ns("node:buffer")
        .doc("Buffer/base64 (node:buffer): atob, btoa.")
        .member(member("atob", Function, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_ATOB", "atob(data: string): string", s::__RTS_FN_NODE_BUFFER_ATOB as *const u8))
        .member(member("btoa", Function, vec![StrPtr], Handle, "__RTS_FN_NODE_BUFFER_BTOA", "btoa(data: string): string", s::__RTS_FN_NODE_BUFFER_BTOA as *const u8))
        .done();
}
