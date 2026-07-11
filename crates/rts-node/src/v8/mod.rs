//! `node:v8` — the structured-clone serializer core: `serialize(value)` →
//! Buffer and `deserialize(buffer)` → value, over RTS's OWN wire format
//! (v8.md §24 — same value-traversal design as the ambient `structuredClone`,
//! NOT V8's ValueSerializer bytes). A real recursive walk of the PolyValue graph
//! (numbers/strings/booleans/null/undefined/arrays/plain objects); functions and
//! symbols are unserializable (a thrown error, matching V8).
//!
//! Deferred (need V8-internal concepts RTS does not have, host-object hooks, or
//! GC/heap-snapshot plumbing): the `Serializer`/`Deserializer` subclassable
//! classes (`writeHeader`/`writeValue`/`releaseBuffer`/`_writeHostObject` +
//! transferArrayBuffer/raw read-write primitives), `getHeapStatistics`/
//! `getHeapSpaceStatistics`/`writeHeapSnapshot` (no V8 heap to introspect),
//! `setFlagsFromString`, `takeCoverage`/`stopCoverage`, `GCProfiler`,
//! `vm.measureMemory`, the startup-snapshot API. Cyclic references (V8 handles
//! them via back-references) are also out of this first cut — a plain DAG
//! round-trips; a cycle would recurse.
//!
//! Layout: `serde` (encode/decode), `symbols` (extern points), `mod`
//! (registration).

mod serde;
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
        intrinsic: None,
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
