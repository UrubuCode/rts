//! `TextDecoderStream` — parity source: `streams.ts`'s
//! `class TextDecoderStream`. A `readable`+`writable` pair whose writable
//! side is `mode::DECODE` (UTF-8-decodes each written bytes-like chunk into a
//! JS string before enqueueing). `label` is accepted and ignored (UTF-8
//! only), matching both the `.ts` and the older git-history Rust.

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::alloc_rtse;

#[rtse::class("TextDecoderStream")]
#[derive(Clone, Default)]
pub struct TextDecoderStream {
    pub(crate) readable: Handle,
    pub(crate) writable: Handle,
}

#[rtse::class("TextDecoderStream")]
impl TextDecoderStream {
    #[rtse::ctor(optional = 1)]
    fn new(_label: Poly) -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        let readable = alloc_rtse("ReadableStream", super::readable::ReadableStream { ctl, locked: false });
        let writable = super::writable::new_linked(ctl, 0, super::writable::mode::DECODE, false);
        TextDecoderStream { readable, writable }
    }

    #[rtse::getter(returns = "ReadableStream")]
    fn readable(self: &TextDecoderStream) -> Handle {
        self.readable
    }

    #[rtse::getter(returns = "WritableStream")]
    fn writable(self: &TextDecoderStream) -> Handle {
        self.writable
    }

    #[rtse::getter]
    fn encoding(self: &TextDecoderStream) -> String {
        "utf-8".to_string()
    }
}
