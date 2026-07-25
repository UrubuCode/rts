//! `text_encoding` — TextEncoder / TextDecoder / atob / btoa / structuredClone
//! / queueMicrotask.
//!
//! The `text_encoding` NAMESPACE (free `encode`/`decode`/`atob`/`btoa`/
//! `structuredClone`/`queueMicrotask` functions) stays hand-written — its externs
//! live in `instance.rs`.
//!
//! The `TextEncoder`/`TextDecoder` global CLASSES (DRAIN_MOTOR §8-9) are fully
//! `#[rtse::class]`-authored: `encoder.rs` (TextEncoder) and `decoder.rs`
//! (TextDecoder — `decode` uses `#[rtse::method(throws, ...)]`, the `throws`
//! keyword that sets `MemberFlags::THROWS` on the generated Member so the
//! `fatal` TypeError path still routes through `try/catch`). No hand-written
//! residual members remain for either class.

pub mod decoder;
pub mod encoder;
pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro de namespace/classe global (helper hand-written, espelha a macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Registra a namespace `text_encoding` no motor (Fase 2 — hand-written, sem
/// macro). Todos os membros são `external` (fn_ptr null).
pub fn register(e: &mut Engine) {
    e.ns("text_encoding")
        .doc("TextEncoder.encode / TextDecoder.decode / atob / btoa / structuredClone / queueMicrotask.")
        .member(m(
            "encode",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_ENCODE",
            "encode(text: string): Uint8Array",
            "Encode string UTF-8 para Buffer handle (Uint8Array semantics).",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "decode",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_DECODE",
            "decode(buf: Uint8Array): string",
            "Decode Buffer handle (bytes UTF-8) para string handle.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "atob",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_ATOB",
            "atob(encoded: string): string",
            "Decode base64 string para string handle.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "btoa",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_BTOA",
            "btoa(data: string): string",
            "Encode string para base64 handle.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "structuredClone",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_STRUCTURED_CLONE",
            "structuredClone(value: any): any",
            "Deep clone de handle (Map, Vec, Buffer, String). Primitivos: usar direto.",
            core::ptr::null::<u8>(),
            false,
        ))
        .member(m(
            "queueMicrotask",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK",
            "queueMicrotask(callback: () => void): void",
            "Executa callback imediatamente (RTS é síncrono, sem microtask queue real).",
            core::ptr::null::<u8>(),
            false,
        ))
        .done();
}

/// Registra a classe global `TextEncoder` — inteiramente `#[rtse::class]`
/// (`encoder.rs`): ctor + `encode` + `encodeInto` cabem 100% na superfície do
/// macro (Handle/&str param+return), sem gap.
pub fn register_text_encoder_class_spec(e: &mut Engine) {
    encoder::register(e);
}

/// Registra a classe global `TextDecoder` — inteiramente `#[rtse::class]`
/// (`decoder.rs`): ctor + `decode` (com `throws`) cabem 100% na superfície do
/// macro, sem workaround de leitura-e-anexo.
pub fn register_text_decoder_class_spec(e: &mut Engine) {
    decoder::register(e);
}
