//! `node:v8` — and the first thing this module owes the reader is that the
//! name is a compatibility fiction. This engine is not V8. It has no
//! generational heap, no named heap spaces, no `ValueSerializer` wire
//! format, no build-flag interpreter, no sampling CPU profiler, no
//! `SnapshotCreator`. Every member below either answers a question this
//! engine's own runtime can honestly answer, or is refused by name with the
//! mechanism it is waiting on — never a V8 number dressed up to look real.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search over `rts-cranelift`'s
//! `src/tags/`, `src/shape/`, `src/abi/`, `src/probe/` turned up nothing that
//! mints a heap-statistics record or a wire-serialize format — both are
//! runtime bookkeeping, not machine capability, so nothing there to call.
//!
//! Inside `rts-core-rwk`: [`rts_core_rwk::entry::Context`] holds `region`
//! (`pub`, `crate::heap::Region`) with `capacity()`/`used()` already on it —
//! that backs [`get_heap_statistics`] below, genuinely, in bytes, via
//! `rts_core_rwk::heap::STRIDE`. `structuredClone`'s deep-clone walk exists
//! (`rts-core-rwk/src/entry/clone.rs`), but it is `pub(super)`: only its
//! *name* is reachable, through the lazy-global lookup a compiled program's
//! own unresolved-identifier path uses — nothing in the crate's public API
//! (`rts_core_rwk::entry::modules`, the one surface this crate may call)
//! hands a caller outside the crate either the callable or the walk itself.
//! `serialize`/`deserialize` are refused below for exactly that reason,
//! named rather than worked around.
//!
//! # Not implemented, by name
//!
//! `getHeapSpaceStatistics`, `getHeapCodeStatistics`, `getCppHeapStatistics`
//! — this engine has one region, not V8's named spaces or a separate cppgc
//! heap; there is nothing to enumerate. `setFlagsFromString` — no flag
//! namespace exists to set into; Cranelift's optimization settings are fixed
//! at this engine's own build time, not a runtime toggle. `takeCoverage` /
//! `stopCoverage` — needs a per-range counter Cranelift's lowering does not
//! emit; a codegen feature, not something reachable from this crate.
//! `setHeapSnapshotNearHeapLimit`, `getHeapSnapshot`, `writeHeapSnapshot` —
//! there is no heap-size soft limit and no snapshot walk; both need new
//! `rts-core-rwk` bookkeeping this crate cannot add to itself.
//! `serialize`/`deserialize`, `Serializer`/`Deserializer`/
//! `DefaultSerializer`/`DefaultDeserializer` — the clone walk they would
//! share is crate-private in `rts-core-rwk` (see Reuse-check above); waits on
//! a public entry point beside [`rts_core_rwk::entry::modules::make_prototype`]
//! that wraps it. `cachedDataVersionTag` — no bytecode/IR cache exists to
//! fingerprint. `isStringOneByteRepresentation` — this engine's strings are
//! always UTF-8 with no second representation to report on; there is no fact
//! here to approximate, only one to refuse. `queryObjects` — needs every live
//! object's originating class walkable by identity; nothing in
//! `rts_core_rwk::entry` enumerates live cells by class. `GCProfiler`,
//! `startCpuProfile`/`SyncCPUProfileHandle`/`CPUProfileHandle`/
//! `HeapProfileHandle` — there is no collector to hook (this engine has no
//! GC yet at all — [`rts_core_rwk::entry::alloc::heap_exhausted`]'s own doc
//! says so) and no sampling profiler. `promiseHooks` — needs a call-site
//! inside the promise settle path this crate does not own.
//! `startupSnapshot` — this engine never enters a snapshot-build mode; there
//! is nothing for `isBuildingSnapshot()` to report but a fixed `false`, and a
//! fixed answer to a question that cannot occur is not implementing the
//! feature, so it is left off rather than shipped as a decoration.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:v8` is — one real function, and nothing invented
/// beside it.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("getHeapStatistics", get_heap_statistics),
        ("serialize", serialize),
        ("deserialize", deserialize),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `v8.getHeapStatistics()`.
///
/// Two of Node's fifteen fields, both real: `total_heap_size` is this
/// engine's single region's fixed capacity, `used_heap_size` is how much of
/// it is handed out — both in bytes, `cells × STRIDE`. The other thirteen
/// (`heap_size_limit`, `malloced_memory`, `number_of_native_contexts`, …)
/// are not emitted at all rather than emitted as `0`: this engine tracks no
/// soft limit, no malloc counter, no realm count, and a `0` in their place
/// would read as a measurement of something that was never taken.
extern "C" fn get_heap_statistics(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let stride = u64::from(rts_core_rwk::heap::STRIDE);
        let total = u64::from(context.region.capacity()) * stride;
        let used = u64::from(context.region.used()) * stride;
        let object = rts_core_rwk::entry::make_object(context);
        let total_v = rts_core_rwk::entry::make_number(total as f64);
        rts_core_rwk::entry::put_member(context, object, "total_heap_size", total_v);
        let used_v = rts_core_rwk::entry::make_number(used as f64);
        rts_core_rwk::entry::put_member(context, object, "used_heap_size", used_v);
        object
    })
}

/// `v8.serialize(value)` — a deep copy, not a byte format.
///
/// # What this answers and what Node answers
///
/// Node answers a `Buffer` holding V8's own wire format, opaque and versioned,
/// which another process running the same V8 can read back. This answers the
/// COPY itself, because the runtime has a deep-copy walk — the one
/// `structuredClone` is — and no wire format at all.
///
/// So the round trip a program actually writes,
/// `deserialize(serialize(x))`, produces what it expects: a value equal to `x`
/// and sharing nothing with it, cycles included. What does NOT work is treating
/// the result as bytes — writing it to a file, sending it over a socket, or
/// reading its `length`. That is the divergence, and it is stated rather than
/// approximated with a `Buffer` whose contents would mean nothing.
///
/// The alternative was leaving both refused. It was rejected because the round
/// trip is what the pair is used for, and a copy is a correct answer to it.
extern "C" fn serialize(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // Outside any borrow: the walk takes and releases its own, because it reads
    // properties and allocates and can do neither while one is held.
    rts_core_rwk::entry::deep_copy(value)
}

/// `v8.deserialize(value)` — the same copy, for the same reason.
///
/// Copying again rather than answering what it was handed: a program calling
/// `deserialize` on a value it kept a reference to must not get that reference
/// back, or the pair would share structure where Node's does not.
extern "C" fn deserialize(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::deep_copy(value)
}
