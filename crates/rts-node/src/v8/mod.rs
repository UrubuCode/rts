//! `node:v8` — the structured-clone serializer core: `serialize(value)` →
//! Buffer and `deserialize(buffer)` → value, over the engine's ONE native
//! pickle primitive (`rts_engine::heap::pickle`, RTSP wire format — the v8.md
//! §5.1 unification; NOT V8's ValueSerializer bytes). Cyclic references and
//! shared identity round-trip via memo back-references, like V8's own format;
//! functions and symbols are unserializable (a thrown error, matching V8).
//!
//! Deferred (need V8-internal concepts RTS does not have, host-object hooks, or
//! GC/heap-snapshot plumbing): the `Serializer`/`Deserializer` subclassable
//! classes (`writeHeader`/`writeValue`/`releaseBuffer`/`_writeHostObject` +
//! transferArrayBuffer/raw read-write primitives), `getHeapStatistics`/
//! `getHeapSpaceStatistics`/`writeHeapSnapshot` (no V8 heap to introspect),
//! `setFlagsFromString`, `takeCoverage`/`stopCoverage`, `GCProfiler`,
//! `vm.measureMemory`, the startup-snapshot API.
//!
//! Layout: `symbols` (extern points), `mod` (registration).

mod symbols;

use rts_engine::AbiType::{self, Handle, PolyValue};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::THROWS,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        emit: None,
    }
}

/// Registers the `node:v8` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:v8")
        .doc("V8 structured-clone serialization (node:v8): serialize, deserialize.")
        .member(func("serialize", vec![PolyValue], Handle, "__RTS_FN_NODE_V8_SERIALIZE", "serialize(value: object): number[]", s::__RTS_FN_NODE_V8_SERIALIZE as *const u8))
        .member(func("deserialize", vec![Handle], PolyValue, "__RTS_FN_NODE_V8_DESERIALIZE", "deserialize(buffer: number[]): object", s::__RTS_FN_NODE_V8_DESERIALIZE as *const u8))
        .done();
}
