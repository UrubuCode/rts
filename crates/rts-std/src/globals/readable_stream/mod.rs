//! Web Streams — ReadableStream / TransformStream family (9 classes).
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Todos os membros são
//! `external` — os externs `__RTS_FN_GL_*STREAM*` vivem em `instance.rs`
//! intactos; aqui só montamos os 9 `register_*_class_spec`. Os getters
//! `writable`/`readable` compartilham os externs
//! `__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE/READABLE` entre 4 classes.

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro de classe global `external` (fn_ptr nulo — o extern vive em
/// `instance.rs`). Espelha o helper hand-written do `error/mod.rs`.
fn m(name: &str, kind: MemberKind, sig: Sig, symbol: &str, ts: &str) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        emit: None,
    }
}

/// Registra a classe global `ReadableStream` (Fase 2 — hand-written, sem macro).
pub fn register_readable_stream_class_spec(e: &mut Engine) {
    e.class("ReadableStream")
        .doc("ReadableStream.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_READABLE_STREAM_NEW",
            "new ReadableStream(underlyingSource?: object): ReadableStream",
        ))
        .member(m(
            "getReader",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_READABLE_STREAM_GET_READER",
            "getReader(): ReadableStreamDefaultReader",
        ))
        .member(m(
            "pipeThrough",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH",
            "pipeThrough(t: { writable: WritableStream; readable: ReadableStream }): ReadableStream",
        ))
        .done();
}

/// Registra a classe global `ReadableStreamDefaultReader`.
pub fn register_reader_class_spec(e: &mut Engine) {
    e.class("ReadableStreamDefaultReader")
        .doc("ReadableStreamDefaultReader.")
        .member(m(
            "read",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_READABLE_STREAM_READER_READ",
            "read(): Promise<{value: any; done: boolean}>",
        ))
        .done();
}

/// Registra a classe global `ReadableStreamDefaultController`.
pub fn register_controller_class_spec(e: &mut Engine) {
    e.class("ReadableStreamDefaultController")
        .doc("ReadableStreamDefaultController.")
        .member(m(
            "enqueue",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE",
            "enqueue(chunk: any): void",
        ))
        .member(m(
            "close",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE",
            "close(): void",
        ))
        .done();
}

/// Registra a classe global `TransformStream`.
pub fn register_transform_stream_class_spec(e: &mut Engine) {
    e.class("TransformStream")
        .doc("TransformStream.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_NEW",
            "new TransformStream(transformer?: object): TransformStream",
        ))
        .member(m(
            "writable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
            "readonly writable: WritableStream",
        ))
        .member(m(
            "readable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
            "readonly readable: ReadableStream",
        ))
        .done();
}

/// Registra a classe global `WritableStream`.
pub fn register_writable_stream_class_spec(e: &mut Engine) {
    e.class("WritableStream")
        .doc("WritableStream.")
        .member(m(
            "getWriter",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_WRITABLE_STREAM_GET_WRITER",
            "getWriter(): WritableStreamDefaultWriter",
        ))
        .done();
}

/// Registra a classe global `WritableStreamDefaultWriter`.
pub fn register_writer_class_spec(e: &mut Engine) {
    e.class("WritableStreamDefaultWriter")
        .doc("WritableStreamDefaultWriter.")
        .member(m(
            "write",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE",
            "write(chunk: any): Promise<void>",
        ))
        .member(m(
            "close",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE",
            "close(): Promise<void>",
        ))
        .done();
}

/// Registra a classe global `TextEncoderStream` (identity passthrough).
pub fn register_text_encoder_stream_class_spec(e: &mut Engine) {
    e.class("TextEncoderStream")
        .doc("TextEncoderStream (identity passthrough).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TEXT_ENCODER_STREAM_NEW",
            "new TextEncoderStream(): TextEncoderStream",
        ))
        .member(m(
            "writable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
            "readonly writable: WritableStream",
        ))
        .member(m(
            "readable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
            "readonly readable: ReadableStream",
        ))
        .done();
}

/// Registra a classe global `TextDecoderStream` (identity passthrough).
pub fn register_text_decoder_stream_class_spec(e: &mut Engine) {
    e.class("TextDecoderStream")
        .doc("TextDecoderStream (identity passthrough).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TEXT_DECODER_STREAM_NEW",
            "new TextDecoderStream(): TextDecoderStream",
        ))
        .member(m(
            "writable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
            "readonly writable: WritableStream",
        ))
        .member(m(
            "readable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
            "readonly readable: ReadableStream",
        ))
        .done();
}

/// Registra a classe global `CompressionStream` — gzip/deflate.
pub fn register_compression_stream_class_spec(e: &mut Engine) {
    e.class("CompressionStream")
        .doc("CompressionStream — gzip/deflate.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_COMPRESSION_STREAM_NEW",
            "new CompressionStream(format: string): CompressionStream",
        ))
        .member(m(
            "writable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
            "readonly writable: WritableStream",
        ))
        .member(m(
            "readable",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
            "readonly readable: ReadableStream",
        ))
        .done();
}
