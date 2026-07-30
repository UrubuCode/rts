//! `node:stream` — Node's streaming interface: `Readable`/`Writable`/`Duplex`/
//! `Transform`/`PassThrough` plus the orchestration functions (`pipeline`/
//! `finished`/`compose`/`duplexPair`/`isErrored`/`isReadable`/`isWritable`/
//! `addAbortSignal`/`getDefaultHighWaterMark`/`setDefaultHighWaterMark`), the
//! `stream/consumers` drainers, and the `stream/promises` forms.
//!
//! Per the spec (docs/node-implementation/stream.md §5.1), this module is a
//! ~100% pure `.ts` state machine over already-primordial engine values
//! (`Array`/`Object`/`Function`/`Promise`/`Map`/`Buffer`) — Node's own
//! `lib/internal/streams/*` has essentially no native binding surface, so RTS
//! implements it as an ambient `.ts` PRELUDE (like `Map`/`Set`/the web streams)
//! rather than as `extern "C"` members. The four `node:stream*` specifiers each
//! register an empty namespace so the module resolver mounts them; the class /
//! function surface is bound to the ambient prelude declarations by the module
//! loader (`node_reexported_prelude` in the engine's flatten pass).
//!
//! `node:stream/web` (WHATWG `ReadableStream`/`WritableStream`/`TransformStream`
//! /…) is the already-existing `rts-shared` `streams.ts` prelude — this module
//! only re-exports those ambient globals, it does not re-implement them.
//!
//! Deferred (documented, not faked): `CompressionStream`/`DecompressionStream`
//! (need the shared zlib/Brotli codec-context externs, tracked with `node:zlib`)
//! and real cross-thread WHATWG stream transfer.

use rts_engine::Engine;

/// The ambient `.ts` prelude implementing the `node:stream` class + function
/// surface, split into cohesive files included IN ORDER (base classes before
/// dependents) by the engine (see `PRELUDE_TS`), after the web `streams.ts`/
/// `events.ts`/buffer preludes they build on.
pub const STREAM_TS: &str = include_str!("stream.ts");
/// The read side (`Readable` + shared read free-functions + `Readable.from`).
pub const STREAM_READABLE_TS: &str = include_str!("readable.ts");
/// The write side (`Writable` + shared write free-functions).
pub const STREAM_WRITABLE_TS: &str = include_str!("writable.ts");
/// `Duplex`/`Transform`/`PassThrough`.
pub const STREAM_DUPLEX_TS: &str = include_str!("duplex.ts");
/// Orchestration (`pipeline`/`finished`/`compose`/…) + consumers + promises.
pub const STREAM_OPS_TS: &str = include_str!("ops.ts");

/// Registers `node:stream` and its three submodule specifiers. All four carry an
/// empty member set: the surface is ambient prelude, not native members (§5.2).
pub fn register(e: &mut Engine) {
    e.ns("node:stream")
        .doc(
            "Streams (node:stream): Readable/Writable/Duplex/Transform/PassThrough \
             + pipeline/finished/compose/duplexPair/isReadable/… (ambient .ts).",
        )
        // Every class/fn is an ambient `.ts` prelude decl of the SAME name — a
        // WHOLE-surface re-export (no native member), so a default import
        // synthesizes a namespace object of these decls.
        .reexport("Stream", "Stream")
        .reexport("Readable", "Readable")
        .reexport("Writable", "Writable")
        .reexport("Duplex", "Duplex")
        .reexport("Transform", "Transform")
        .reexport("PassThrough", "PassThrough")
        .reexport("pipeline", "pipeline")
        .reexport("finished", "finished")
        .reexport("compose", "compose")
        .reexport("duplexPair", "duplexPair")
        .reexport("isErrored", "isErrored")
        .reexport("isReadable", "isReadable")
        .reexport("isWritable", "isWritable")
        .reexport("addAbortSignal", "addAbortSignal")
        .reexport("getDefaultHighWaterMark", "getDefaultHighWaterMark")
        .reexport("setDefaultHighWaterMark", "setDefaultHighWaterMark")
        .done();
    // Submodule decls are prefixed to avoid clashing with the base module's
    // `pipeline`/`finished` (callback vs promise form).
    e.ns("node:stream/promises")
        .doc("Streams promise API (node:stream/promises): pipeline/finished.")
        .reexport("pipeline", "__streamPromisesPipeline")
        .reexport("finished", "__streamPromisesFinished")
        .done();
    e.ns("node:stream/consumers")
        .doc(
            "Stream consumers (node:stream/consumers): arrayBuffer/blob/buffer/ \
             bytes/json/text — drain a stream into one value.",
        )
        .reexport("text", "__streamConsumersText")
        .reexport("json", "__streamConsumersJson")
        .reexport("buffer", "__streamConsumersBuffer")
        .reexport("arrayBuffer", "__streamConsumersArrayBuffer")
        .reexport("bytes", "__streamConsumersBytes")
        .reexport("blob", "__streamConsumersBlob")
        .done();
    e.ns("node:stream/web")
        .doc(
            "WHATWG streams (node:stream/web): ReadableStream/WritableStream/ \
             TransformStream/… — re-exports the ambient web-stream globals.",
        )
        // Only the classes the `rts-shared` `streams.ts` prelude provides.
        .reexport("ReadableStream", "ReadableStream")
        .reexport("WritableStream", "WritableStream")
        .reexport("TransformStream", "TransformStream")
        .reexport("TextEncoderStream", "TextEncoderStream")
        .reexport("TextDecoderStream", "TextDecoderStream")
        .done();
}
