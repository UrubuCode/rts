//! `text_encoding` — TextEncoder / TextDecoder / atob / btoa / structuredClone
//! / queueMicrotask.
//!
//! Migrado do `#[rts_namespace]` + `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Todos os
//! membros são `external`: os externs `__RTS_FN_GL_TEXTENC_*` /
//! `__RTS_FN_GL_TEXTDEC_*` ficam em `instance.rs` intactos; aqui só
//! registramos a namespace `SPEC` + os dois `*_CLASS_SPEC` (fn_ptr null, sem
//! reemitir `#[no_mangle]`).

pub mod instance;

use rts_engine::{AbiType, DefaultArg, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

/// Registra a classe global `TextEncoder` no motor (hand-written, sem macro).
/// Membros `external` (fn_ptr null).
pub fn register_text_encoder_class_spec(e: &mut Engine) {
    e.class("TextEncoder")
        .doc("TextEncoder — encode string para UTF-8 Uint8Array.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_NEW",
            "new TextEncoder()",
            "new TextEncoder() — sem args (sempre UTF-8).",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "encode",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_TEXTENC_ENCODE_INSTANCE",
            "encode(text: string): Uint8Array",
            "encoder.encode(text) — UTF-8 bytes como Buffer handle.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "encodeInto",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_TEXTENC_ENCODE_INTO",
            "encodeInto(src: string, dest: Uint8Array): TextEncoderEncodeIntoResult",
            "encoder.encodeInto(src, dest) — escreve UTF-8 em dest (só code points inteiros que cabem); retorna {read, written}.",
            core::ptr::null::<u8>(),
            false,
        ))
        .done();
}

/// Registra a classe global `TextDecoder` no motor (hand-written, sem macro).
/// Membros `external` (fn_ptr null).
pub fn register_text_decoder_class_spec(e: &mut Engine) {
    e.class("TextDecoder")
        .doc("TextDecoder — decode Uint8Array UTF-8 para string.")
        .member(m(
            "new",
            MemberKind::Constructor,
            // `label` and the `{fatal, ignoreBOM}` options bag are both OPTIONAL
            // (`new TextDecoder()` valid); default them to `undefined` so the
            // generic ctor resolver admits 0/1/2-arg calls. UTF-8 only — the
            // label is accepted and ignored; the options land in instance state.
            Sig::with_defaults(
                vec![AbiType::StrPtr, AbiType::Handle],
                AbiType::Handle,
                vec![DefaultArg::Undefined, DefaultArg::Undefined],
            ),
            "__RTS_FN_GL_TEXTDEC_NEW",
            "new TextDecoder(label?: string, options?: TextDecoderOptions)",
            "new TextDecoder(label?, {fatal?, ignoreBOM?}?) — so' UTF-8; options guardadas na instância.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member({
            let mut mem = m(
                "decode",
                MemberKind::InstanceMethod,
                // `buf` optional (a no-arg call FLUSHES a streaming decoder) and
                // an optional `{stream}` options bag; defaults cover ALL sig
                // slots (receiver included — the front drops slot 0).
                Sig::with_defaults(
                    vec![AbiType::Handle, AbiType::Handle, AbiType::Handle],
                    AbiType::Handle,
                    vec![
                        DefaultArg::Undefined,
                        DefaultArg::Undefined,
                        DefaultArg::Undefined,
                    ],
                ),
                "__RTS_FN_GL_TEXTDEC_DECODE_INSTANCE",
                "decode(buf?: Uint8Array, options?: TextDecodeOptions): string",
                "decoder.decode(buf?, {stream?}?) — BOM por stream, carry de UTF-8 dividido, fatal lança TypeError.",
                core::ptr::null::<u8>(),
                false,
            );
            // `{fatal: true}` rejeita input malformado com TypeError pendente —
            // o front emite o post-call error check ao ver esta flag.
            mem.flags = MemberFlags::THROWS;
            mem
        })
        .done();
}
