//! Web Streams — ReadableStream / TransformStream family.
//!
//! Registered as composite/plain GlobalClassSpecs so codegen can resolve
//! `new ReadableStream({...})` via `global_class_lookup("ReadableStream")`
//! and dispatch instance methods (`getReader`, `read`, `enqueue`, `close`,
//! `getWriter`, `write`) through the standard global-class path.
//!
//! Data model (all handles are GC `Entry::Map`):
//! - stream:     `__buf` (Vec handle), `__closed` (0/1), `__transform` (Function|0)
//! - controller: `__stream` (stream handle)
//! - reader:     `__stream`, `__cursor` (i64)
//! - writer:     `__stream`
//!
//! `reader.read()` returns a *resolved* Promise of `{value, done}`; the
//! cooperative `await` (default) drains it. Data is synchronous (the producer
//! enqueues into the buffer before `read()` runs).

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

const fn method(
    name: &'static str,
    symbol: &'static str,
    args: &'static [AbiType],
    returns: AbiType,
    ts: &'static str,
) -> NamespaceMember {
    NamespaceMember {
        name,
        kind: MemberKind::InstanceMethod,
        symbol,
        args,
        returns,
        doc: "Web Streams instance method.",
        ts_signature: ts,
        intrinsic: None,
        pure: false,
    }
}

const fn ctor(symbol: &'static str, sig: &'static str) -> NamespaceMember {
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol,
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Web Streams constructor.",
        ts_signature: sig,
        intrinsic: None,
        pure: false,
    }
}

// ── ReadableStream ────────────────────────────────────────────────────────────

pub const READABLE_STREAM_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_READABLE_STREAM_NEW",
        "new ReadableStream(underlyingSource?: object): ReadableStream",
    ),
    method(
        "getReader",
        "__RTS_FN_GL_READABLE_STREAM_GET_READER",
        &[AbiType::Handle],
        AbiType::Handle,
        "getReader(): ReadableStreamDefaultReader",
    ),
    method(
        "pipeThrough",
        "__RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH",
        &[AbiType::Handle, AbiType::Handle],
        AbiType::Handle,
        "pipeThrough(t: { writable: WritableStream; readable: ReadableStream }): ReadableStream",
    ),
];

// ── TextEncoderStream / TextDecoderStream ───────────────────────────────────────
// Modeled as identity passthrough streams (encode-then-decode round-trips to the
// original text). `.writable`/`.readable` reuse the TransformStream side getters
// (same Map shape). pipeThrough shares the upstream buffer downstream.

pub const TEXT_ENCODER_STREAM_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_TEXT_ENCODER_STREAM_NEW",
        "new TextEncoderStream(): TextEncoderStream",
    ),
    getter(
        "writable",
        "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        "readonly writable: WritableStream",
    ),
    getter(
        "readable",
        "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        "readonly readable: ReadableStream",
    ),
];

pub const TEXT_ENCODER_STREAM_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TextEncoderStream",
    doc: "TextEncoderStream (identity passthrough model).",
    members: TEXT_ENCODER_STREAM_MEMBERS,
};

pub const TEXT_DECODER_STREAM_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_TEXT_DECODER_STREAM_NEW",
        "new TextDecoderStream(): TextDecoderStream",
    ),
    getter(
        "writable",
        "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        "readonly writable: WritableStream",
    ),
    getter(
        "readable",
        "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        "readonly readable: ReadableStream",
    ),
];

pub const TEXT_DECODER_STREAM_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TextDecoderStream",
    doc: "TextDecoderStream (identity passthrough model).",
    members: TEXT_DECODER_STREAM_MEMBERS,
};

pub const READABLE_STREAM_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "ReadableStream",
    doc: "ReadableStream (Web Streams, synchronous-buffer model).",
    members: READABLE_STREAM_MEMBERS,
};

// ── ReadableStreamDefaultReader ────────────────────────────────────────────────

pub const READER_MEMBERS: &[NamespaceMember] = &[
    method(
        "read",
        "__RTS_FN_GL_READABLE_STREAM_READER_READ",
        &[AbiType::Handle],
        AbiType::Handle,
        "read(): Promise<{value: any; done: boolean}>",
    ),
];

pub const READER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "ReadableStreamDefaultReader",
    doc: "ReadableStreamDefaultReader.",
    members: READER_MEMBERS,
};

// ── ReadableStreamDefaultController ─────────────────────────────────────────────

pub const CONTROLLER_MEMBERS: &[NamespaceMember] = &[
    method(
        "enqueue",
        "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE",
        &[AbiType::Handle, AbiType::Handle],
        AbiType::Void,
        "enqueue(chunk: any): void",
    ),
    method(
        "close",
        "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE",
        &[AbiType::Handle],
        AbiType::Void,
        "close(): void",
    ),
];

pub const CONTROLLER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "ReadableStreamDefaultController",
    doc: "ReadableStreamDefaultController.",
    members: CONTROLLER_MEMBERS,
};

// ── TransformStream ────────────────────────────────────────────────────────────

const fn getter(
    name: &'static str,
    symbol: &'static str,
    ts: &'static str,
) -> NamespaceMember {
    NamespaceMember {
        name,
        kind: MemberKind::InstanceGetter,
        symbol,
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Web Streams side accessor.",
        ts_signature: ts,
        intrinsic: None,
        pure: false,
    }
}

pub const TRANSFORM_STREAM_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_TRANSFORM_STREAM_NEW",
        "new TransformStream(transformer?: object): TransformStream",
    ),
    // Typed sides so `ts.writable.getWriter()` / `ts.readable.getReader()`
    // dispatch as InstanceMethods (a method call on an UNtyped getter result is
    // not supported by codegen). Both return the same shared stream handle; the
    // static type just selects the writer-vs-reader method set.
    getter(
        "writable",
        "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        "readonly writable: WritableStream",
    ),
    getter(
        "readable",
        "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        "readonly readable: ReadableStream",
    ),
];

pub const TRANSFORM_STREAM_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TransformStream",
    doc: "TransformStream (Web Streams, synchronous-buffer model).",
    members: TRANSFORM_STREAM_MEMBERS,
};

// ── WritableStream (the `.writable` side of a TransformStream) ──────────────────

pub const WRITABLE_STREAM_MEMBERS: &[NamespaceMember] = &[
    method(
        "getWriter",
        "__RTS_FN_GL_WRITABLE_STREAM_GET_WRITER",
        &[AbiType::Handle],
        AbiType::Handle,
        "getWriter(): WritableStreamDefaultWriter",
    ),
];

pub const WRITABLE_STREAM_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "WritableStream",
    doc: "WritableStream.",
    members: WRITABLE_STREAM_MEMBERS,
};

// ── WritableStreamDefaultWriter ─────────────────────────────────────────────────

pub const WRITER_MEMBERS: &[NamespaceMember] = &[
    method(
        "write",
        "__RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE",
        &[AbiType::Handle, AbiType::Handle],
        AbiType::Handle,
        "write(chunk: any): Promise<void>",
    ),
    method(
        "close",
        "__RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE",
        &[AbiType::Handle],
        AbiType::Handle,
        "close(): Promise<void>",
    ),
];

pub const WRITER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "WritableStreamDefaultWriter",
    doc: "WritableStreamDefaultWriter.",
    members: WRITER_MEMBERS,
};
