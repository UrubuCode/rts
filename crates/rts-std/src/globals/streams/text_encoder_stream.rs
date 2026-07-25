//! `TextEncoderStream` — parity source: `streams.ts`'s
//! `class TextEncoderStream`. A `readable`+`writable` pair whose writable
//! side is `mode::ENCODE` (UTF-8-encodes each written chunk's JS-string form
//! into a plain `number[]` byte array before enqueueing).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::alloc_rtse;

#[rtse::class("TextEncoderStream")]
#[derive(Clone, Default)]
pub struct TextEncoderStream {
    pub(crate) readable: Handle,
    pub(crate) writable: Handle,
}

#[rtse::class("TextEncoderStream")]
impl TextEncoderStream {
    #[rtse::ctor]
    fn new() -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        let readable = alloc_rtse("ReadableStream", super::readable::ReadableStream { ctl, locked: false });
        let writable = super::writable::new_linked(ctl, 0, super::writable::mode::ENCODE, false);
        TextEncoderStream { readable, writable }
    }

    #[rtse::getter(returns = "ReadableStream")]
    fn readable(self: &TextEncoderStream) -> Handle {
        self.readable
    }

    #[rtse::getter(returns = "WritableStream")]
    fn writable(self: &TextEncoderStream) -> Handle {
        self.writable
    }

    #[rtse::getter]
    fn encoding(self: &TextEncoderStream) -> String {
        "utf-8".to_string()
    }
}
