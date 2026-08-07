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
//! that backs [`get_heap_statistics`] and [`get_heap_space_statistics`] below,
//! genuinely, in bytes, via `rts_core_rwk::heap::STRIDE`. Both read the SAME
//! two numbers rather than each deriving its own, because two derivations of
//! one measurement is how the two functions would come to disagree about how
//! full the heap is. `structuredClone`'s deep-clone walk exists
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
//! `getHeapCodeStatistics`, `getCppHeapStatistics` — the first needs the size
//! of the compiled-code region (the JIT's executable pages, or the linked
//! binary's `.text`), which is the host's and the machine's and is reachable
//! from neither `rts_core_rwk::entry::modules` nor here; the second needs a
//! separate cppgc-analogue heap this engine does not have at all. Emitting
//! four or six zeroes in their place would be a measurement of something that
//! was never taken, which is the one thing this module refuses hardest.
//! `setFlagsFromString` — no flag
//! namespace exists to set into; Cranelift's optimization settings are fixed
//! at this engine's own build time, not a runtime toggle.
//! `setHeapSnapshotNearHeapLimit`, `getHeapSnapshot`, `writeHeapSnapshot` —
//! there is no heap-size soft limit and no snapshot walk; both need new
//! `rts-core-rwk` bookkeeping this crate cannot add to itself.
//! `serialize`/`deserialize`, `Serializer`/`Deserializer`/
//! `DefaultSerializer`/`DefaultDeserializer` — the clone walk they would
//! share is crate-private in `rts-core-rwk` (see Reuse-check above); waits on
//! a public entry point beside [`rts_core_rwk::entry::modules::make_prototype`]
//! that wraps it. `queryObjects` — needs every live
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
//!
//! # Divergences in what IS here
//!
//! Refusing by name is not the only way to be dishonest, so the members below
//! that answer something state where their answer differs from Node's:
//! [`get_heap_statistics`] emits five of Node's fifteen fields and omits the
//! other ten, [`get_heap_space_statistics`] enumerates one honestly-named
//! space instead of V8's seven and omits `physical_space_size`,
//! [`cached_data_version_tag`] fingerprints a build rather than a bytecode
//! cache, and [`is_string_one_byte_representation`] answers `undefined`
//! instead of throwing `TypeError` for a non-string, because
//! `rts_core_rwk::entry::throw` ends the PROGRAM rather than raising a
//! catchable error — the divergence every other module in this crate records
//! for the same reason.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:v8` is — what this engine can honestly answer, and
/// nothing invented beside it.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("getHeapStatistics", get_heap_statistics),
        ("getHeapSpaceStatistics", get_heap_space_statistics),
        ("cachedDataVersionTag", cached_data_version_tag),
        ("isStringOneByteRepresentation", is_string_one_byte_representation),
        ("takeCoverage", take_coverage),
        ("stopCoverage", stop_coverage),
        ("serialize", serialize),
        ("deserialize", deserialize),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// How large the one region is and how much of it is handed out, in bytes.
///
/// The single source both statistics functions read. It takes the context
/// rather than reaching for it because both callers ask from inside a borrow,
/// which is the rule this crate states: a helper that can only be called
/// correctly beats one each caller has to remember the rules for.
fn region_bytes(context: &Context) -> (f64, f64) {
    let stride = u64::from(rts_core_rwk::heap::STRIDE);
    let total = u64::from(context.region.capacity()) * stride;
    let used = u64::from(context.region.used()) * stride;
    (total as f64, used as f64)
}

/// `v8.getHeapStatistics()`.
///
/// Five of Node's fifteen fields, every one of them a real measurement of this
/// engine's single region rather than a V8 field name with a number under it:
///
/// - `total_heap_size` — the region's capacity, in bytes (`cells × STRIDE`).
/// - `used_heap_size` — how much of it is handed out.
/// - `total_available_size` — the difference, which is the same measurement
///   subtracted rather than a second count of free space.
/// - `heap_size_limit` — equal to `total_heap_size`, and this is a FACT here
///   rather than a placeholder: the region does not grow, so the size it can
///   reach and the size it has are the same number. A program comparing
///   `used_heap_size` against it to decide it is near exhaustion gets the
///   right answer.
/// - `total_physical_size` — also equal, for the same reason: the region is
///   committed at its capacity, so nothing of it is reserved-but-unbacked.
///
/// The other ten (`malloced_memory`, `peak_malloced_memory`,
/// `does_zap_garbage`, `number_of_native_contexts`, `external_memory`, …) are
/// not emitted at all rather than emitted as `0`: this engine tracks no malloc
/// counter, no realm count and no external-memory total, and a `0` in their
/// place would read as a measurement of something that was never taken. A
/// program can tell an absent field from a measured zero with `in`; it cannot
/// tell a fabricated zero from anything.
extern "C" fn get_heap_statistics(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let (total, used) = region_bytes(context);
        let object = rts_core_rwk::entry::make_object(context);
        let fields = [
            ("total_heap_size", total),
            ("used_heap_size", used),
            ("total_available_size", total - used),
            ("heap_size_limit", total),
            ("total_physical_size", total),
        ];
        for (name, value) in fields {
            let held = rts_core_rwk::entry::make_number(value);
            rts_core_rwk::entry::put_member(context, object, name, held);
        }
        object
    })
}

/// `v8.getHeapSpaceStatistics()`.
///
/// One entry, named `rts_region`, not `new_space`/`old_space`/`code_space`.
///
/// The alternative was refusing the member entirely — which is what this
/// module did until now, on the reasoning that there are no V8 spaces to
/// enumerate. That lost because the question a program actually asks here is
/// "how is the heap divided and how full is each part", and this engine has a
/// true answer to it: one part, this full. Answering it under V8's space names
/// would have been the dishonest version, because a caller keying off
/// `space_name === 'old_space'` would then be told a generational heap exists.
/// It does not, and the name says so.
///
/// `physical_space_size` is omitted for the reason the ten omitted fields
/// above are omitted; `space_available_size` is real, being capacity minus
/// used.
extern "C" fn get_heap_space_statistics(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let (total, used) = region_bytes(context);
        let space = rts_core_rwk::entry::make_object(context);
        let name = rts_core_rwk::entry::make_string(context, "rts_region");
        rts_core_rwk::entry::put_member(context, space, "space_name", name);
        let fields = [
            ("space_size", total),
            ("space_used_size", used),
            ("space_available_size", total - used),
        ];
        for (field, value) in fields {
            let held = rts_core_rwk::entry::make_number(value);
            rts_core_rwk::entry::put_member(context, space, field, held);
        }
        rts_core_rwk::entry::make_array_in(context, vec![space])
    })
}

/// `v8.cachedDataVersionTag()` — a build fingerprint, which is what the tag IS
/// for even though what it guards does not exist here.
///
/// Node's tag answers "would bytecode produced by another process be safe to
/// consume in this one". This engine has no bytecode cache, so nothing is
/// guarded — but the tag's CONTRACT is still satisfiable and still useful:
/// equal tags mean the same build, different tags mean a different one. It is
/// derived at compile time from this crate's version and the target triple's
/// components, so it changes exactly when the produced code could.
///
/// The alternative was refusing it, as this module did until now. That lost
/// because the value is not an estimate of an unmeasurable quantity — it is a
/// complete, correct answer to a smaller question, and the doc says which
/// question. What would have been dishonest is returning a V8 version number.
extern "C" fn cached_data_version_tag(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    // FNV-1a over the build's identity, in a `const fn` so the number is baked
    // rather than recomputed per call — and so it is impossible for two calls
    // in one process to disagree.
    const fn fold(mut hash: u32, text: &str) -> u32 {
        let bytes = text.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            hash ^= bytes[at] as u32;
            hash = hash.wrapping_mul(16_777_619);
            at += 1;
        }
        hash
    }
    const TAG: u32 = fold(
        fold(
            fold(2_166_136_261, env!("CARGO_PKG_VERSION")),
            std::env::consts::ARCH,
        ),
        std::env::consts::OS,
    );
    rts_core_rwk::entry::make_number(f64::from(TAG))
}

/// `v8.isStringOneByteRepresentation(content)`.
///
/// # Why this is a measurement here and an approximation in the spec doc
///
/// `docs/reference/node/v8.md` §5.1 calls this an approximation of a
/// V8-internal fact. It is not, in this engine, and calling it one would
/// understate what is known: this engine stores every string as UTF-8 and has
/// no second representation, so "is this string held one byte per character"
/// has a decidable answer — it is held one byte per character exactly when
/// every byte of its UTF-8 encoding is below `0x80`. That is a property of the
/// bytes actually in the heap, not a guess about a storage decision made
/// elsewhere.
///
/// The divergence from Node is the opposite direction from the usual one: V8
/// may answer `false` for a pure-ASCII string it happens to keep as UTF-16,
/// depending on how the string was built. This answers by content, so it is
/// deterministic where V8's is historical.
///
/// `undefined` for a non-string, where Node throws `TypeError`. Asking WHAT
/// the argument is uses `string_in` rather than `text_of`: `text_of` is
/// `ToString` and would answer `"42"` for the number `42`, reporting a
/// one-byte representation for a value that is not a string at all — the
/// coercion-as-type-test defect three modules in this crate already shipped.
extern "C" fn is_string_one_byte_representation(
    _e: u64,
    _this: u64,
    content: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        match rts_core_rwk::entry::string_in(context, content) {
            Some(text) => rts_core_rwk::entry::boolean_value(text.is_ascii()),
            None => rts_core_rwk::entry::undefined_in(context),
        }
    })
}

/// `v8.takeCoverage()` — nothing to write, which is Node's own documented
/// behaviour here rather than a stub standing in for one.
///
/// Node writes collected coverage to disk ONLY when the process was started
/// with `NODE_V8_COVERAGE=<dir>`; without it the call is a documented no-op
/// that returns `undefined`. This engine never collects coverage — the
/// per-range counters would have to be emitted by `rts-codegen`'s lowering,
/// which this crate does not own — so the no-op path is the only path, and it
/// is the same observable behaviour a real Node run without that variable set
/// produces.
///
/// This is the one place in the module where doing nothing is a correct
/// answer rather than a hidden gap, and it is correct for a stateable reason:
/// the function returns no data, so there is no data to fabricate. Were it to
/// answer a coverage REPORT, an empty one would be a lie and it would be
/// refused instead.
extern "C" fn take_coverage(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::undefined_value()
}

/// `v8.stopCoverage()` — the same, for the same reason.
///
/// Stops a collection that was never running. Node's own `stopCoverage`
/// writes nothing either; persisting is `takeCoverage`'s job, and both are
/// no-ops without `NODE_V8_COVERAGE`.
extern "C" fn stop_coverage(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::undefined_value()
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
