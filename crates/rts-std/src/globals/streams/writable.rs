//! `WritableStream` — folds the `.ts`'s internal `__StreamSink` DIRECTLY (no
//! separate class; nothing in JS ever constructs a bare sink). `mode` drives
//! `write()`/`close()` (both pub(crate) plain fns, NOT JS members — same as
//! the spec, where `write`/`close` live on the WRITER, not the stream).
//!
//! `getWriter()` is a HAND-WRITTEN residual (not `#[rtse::method]`): the
//! `WritableStreamDefaultWriter` it returns must hold THIS stream's own
//! handle (so a later `writer.write(x)` mutates the SAME instance — the
//! `compress` mode's byte accumulator needs that), but a normal
//! `#[rtse::method]` body only ever sees a CLONE of the receiver, with no way
//! to recover the handle it was cloned from. Same pattern as
//! `abort::signal`'s `AbortSignal.abort/timeout/any` statics.

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::{Entry, alloc_entry, alloc_rtse, with_rtse, with_rtse_mut};
use rts_engine::heap::shapes::handle_word_auto;
use rts_engine::{Engine, Member, MemberFlags, MemberKind, Sig};

/// `write()`/`close()` dispatch tag.
pub(crate) mod mode {
    pub const IDENTITY: i64 = 0;
    pub const TRANSFORM: i64 = 1;
    pub const ENCODE: i64 = 2;
    pub const DECODE: i64 = 3;
    pub const COMPRESS: i64 = 4;
}

#[rtse::class("WritableStream")]
#[derive(Clone, Default)]
pub struct WritableStream {
    pub(crate) ctl: Handle,
    /// The transformer/underlying-sink object (`Poly`, `0` = none) — only
    /// consulted in `mode::TRANSFORM`.
    pub(crate) transformer: u64,
    pub(crate) mode: i64,
    /// Raw byte accumulator, used only in `mode::COMPRESS`.
    pub(crate) accum: Vec<u8>,
    pub(crate) deflate: bool,
    #[rtse::variable]
    pub(crate) locked: bool,
}

#[rtse::class("WritableStream")]
impl WritableStream {
    /// `new WritableStream(sink?)` — a plain writable has no sink (identity
    /// passthrough); an object arg is treated as a transformer (matches the
    /// `.ts`'s approximate model: `s.__t = sink; s.__mode = "transform";`).
    #[rtse::ctor(optional = 1)]
    fn new(sink: Poly) -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        if super::is_nullish(sink) {
            WritableStream { ctl, transformer: 0, mode: mode::IDENTITY, accum: Vec::new(), deflate: false, locked: false }
        } else {
            WritableStream { ctl, transformer: sink, mode: mode::TRANSFORM, accum: Vec::new(), deflate: false, locked: false }
        }
    }
}

/// `writer.write(chunk)` — dispatches on `mode`. `transform` calls
/// `transformer.transform(chunk, controller)` if present, else enqueues
/// directly; `encode`/`decode` convert; `compress` buffers raw bytes.
pub(crate) fn write_into(h: u64, chunk: u64) {
    let st = with_rtse::<WritableStream, _>(h, |s| s.cloned());
    let Some(st) = st else { return };
    match st.mode {
        mode::TRANSFORM => {
            if st.transformer != 0 {
                let handled =
                    super::call_if_present(st.transformer, "transform", &[chunk, handle_word_auto(st.ctl)]).is_some();
                if handled {
                    return;
                }
            }
            super::controller::enqueue_into(st.ctl, chunk);
        }
        mode::ENCODE => {
            let s = super::js_to_string(chunk);
            let arr = super::bytes_to_array(s.as_bytes());
            super::controller::enqueue_into(st.ctl, handle_word_auto(arr));
        }
        mode::DECODE => {
            let bytes = super::chunk_bytes(chunk);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let sh = alloc_entry(Entry::String(text.into_bytes()));
            super::controller::enqueue_into(st.ctl, handle_word_auto(sh));
        }
        mode::COMPRESS => {
            let bytes = super::chunk_bytes(chunk);
            with_rtse_mut::<WritableStream, _>(h, |s| {
                if let Some(s) = s {
                    s.accum.extend_from_slice(&bytes);
                }
            });
        }
        _ => {
            super::controller::enqueue_into(st.ctl, chunk);
        }
    }
}

/// `writer.close()` — runs `transformer.flush(controller)` (transform mode)
/// or finalizes the gzip/deflate accumulator (compress mode), then closes
/// the underlying controller (forwarding the close downstream if piped).
pub(crate) fn close_of(h: u64) {
    let st = with_rtse::<WritableStream, _>(h, |s| s.cloned());
    let Some(st) = st else { return };
    if st.mode == mode::TRANSFORM && st.transformer != 0 {
        super::call_if_present(st.transformer, "flush", &[handle_word_auto(st.ctl)]);
    }
    if st.mode == mode::COMPRESS {
        let compressed = if st.deflate {
            super::compression_stream::deflate_bytes(&st.accum)
        } else {
            super::compression_stream::gzip_bytes(&st.accum)
        };
        let buf_h = alloc_entry(Entry::Buffer(compressed));
        super::controller::enqueue_into(st.ctl, handle_word_auto(buf_h));
    }
    super::controller::close_of(st.ctl);
}

/// Construct a `WritableStream` instance directly (bypassing the JS ctor) —
/// used by `TransformStream`/`TextEncoderStream`/`TextDecoderStream`/
/// `CompressionStream`, which need a writable side wired to a SPECIFIC,
/// already-allocated controller (shared with their readable side).
pub(crate) fn new_linked(ctl: u64, transformer: u64, m: i64, deflate: bool) -> u64 {
    alloc_rtse(
        "WritableStream",
        WritableStream { ctl, transformer, mode: m, accum: Vec::new(), deflate, locked: false },
    )
}

/// `stream.getWriter()` — hand-written (needs `recv`, the stream's OWN
/// handle; see the module doc comment).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WRITABLESTREAM_GET_WRITER(recv: u64) -> u64 {
    alloc_rtse(
        "WritableStreamDefaultWriter",
        super::writer::WritableStreamDefaultWriter { stream: recv },
    )
}

/// Registers `WritableStream` (macro members) then appends the hand-written
/// `getWriter` — same reopening pattern as `abort::signal`.
pub fn register_writable_stream_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e.registry().class("WritableStream").map(|c| c.members.clone()).unwrap_or_default();
    let mut cb = e.class("WritableStream").doc("WritableStream.");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(Member {
        name: "getWriter".into(),
        kind: MemberKind::InstanceMethod,
        sig: Sig::new(vec![rts_engine::AbiType::Handle], rts_engine::AbiType::Handle),
        symbol: "__RTS_FN_GL_WRITABLESTREAM_GET_WRITER".into(),
        fn_ptr: rts_engine::FnPtr(__RTS_FN_GL_WRITABLESTREAM_GET_WRITER as *const u8),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: "getWriter(): WritableStreamDefaultWriter".into(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    })
    .done();
}
