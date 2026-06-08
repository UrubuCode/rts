//! `text_encoding` — TextEncoder / TextDecoder / atob / btoa / structuredClone
//! / queueMicrotask. Migrado ao modelo `#[rts_namespace]` + `#[rts_class]`
//! (stage 2c) via membros `external`: os externs `__RTS_FN_GL_TEXTENC_*` /
//! `__RTS_FN_GL_TEXTDEC_*` ficam em `instance.rs` intactos; os macros derivam
//! apenas o `SPEC` + os dois `*_CLASS_SPEC`.

pub mod instance;

#[allow(unused_imports)]
use rts_abi::ty::{Handle, Str, U64};
use rts_macro::{rts_class, rts_namespace};

/// TextEncoder.encode / TextDecoder.decode / atob / btoa / structuredClone / queueMicrotask.
#[rts_namespace(text_encoding, sym = "GL_TEXTENC")]
impl TextEncodingNs {
    /// Encode string UTF-8 para Buffer handle (Uint8Array semantics).
    #[rts_fn(external, ts = "encode(text: string): Uint8Array", pure)]
    pub fn encode(_text: Str) -> Handle {
        unreachable!()
    }

    /// Decode Buffer handle (bytes UTF-8) para string handle.
    #[rts_fn(external, ts = "decode(buf: Uint8Array): string", pure)]
    pub fn decode(_buf: Handle) -> Handle {
        unreachable!()
    }

    /// Decode base64 string para string handle.
    #[rts_fn(external, ts = "atob(encoded: string): string", pure)]
    pub fn atob(_encoded: Str) -> Handle {
        unreachable!()
    }

    /// Encode string para base64 handle.
    #[rts_fn(external, ts = "btoa(data: string): string", pure)]
    pub fn btoa(_data: Str) -> Handle {
        unreachable!()
    }

    /// Deep clone de handle (Map, Vec, Buffer, String). Primitivos: usar direto.
    #[rts_fn(
        external,
        name = "structuredClone",
        ts = "structuredClone(value: any): any"
    )]
    pub fn structured_clone(_value: Handle) -> Handle {
        unreachable!()
    }

    /// Executa callback imediatamente (RTS é síncrono, sem microtask queue real).
    #[rts_fn(
        external,
        name = "queueMicrotask",
        ts = "queueMicrotask(callback: () => void): void"
    )]
    pub fn queue_microtask(_callback: U64) {
        unreachable!()
    }
}

/// TextEncoder — encode string para UTF-8 Uint8Array.
#[rts_class(TextEncoder, prefix = "TEXTENC", spec = "TEXT_ENCODER_CLASS_SPEC")]
impl TextEncoderClass {
    /// new TextEncoder() — sem args (sempre UTF-8).
    #[rts_ctor(external, ts = "new TextEncoder()", pure)]
    pub fn new() -> Handle {
        unreachable!()
    }

    /// encoder.encode(text) — UTF-8 bytes como Buffer handle.
    #[rts_method(
        external,
        name = "encode",
        symbol = "__RTS_FN_GL_TEXTENC_ENCODE_INSTANCE",
        ts = "encode(text: string): Uint8Array",
        pure
    )]
    pub fn encode_instance(_recv: Handle, _text: Str) -> Handle {
        unreachable!()
    }
}

/// TextDecoder — decode Uint8Array UTF-8 para string.
#[rts_class(TextDecoder, prefix = "TEXTDEC", spec = "TEXT_DECODER_CLASS_SPEC")]
impl TextDecoderClass {
    /// new TextDecoder(label?) — label opcional (so' UTF-8 suportado, label ignorado).
    #[rts_ctor(external, ts = "new TextDecoder(label?: string)", pure)]
    pub fn new(_label: Str) -> Handle {
        unreachable!()
    }

    /// decoder.decode(buf) — Buffer handle para string handle.
    #[rts_method(
        external,
        name = "decode",
        symbol = "__RTS_FN_GL_TEXTDEC_DECODE_INSTANCE",
        ts = "decode(buf: Uint8Array): string",
        pure
    )]
    pub fn decode_instance(_recv: Handle, _buf: Handle) -> Handle {
        unreachable!()
    }
}
