//! `ReadableStreamDefaultController` — passed to `start(controller)` /
//! `transform(chunk, controller)`; the only way user JS enqueues chunks into
//! a `ReadableStream`. Parity source: `streams.ts`'s `class RSDController`.
//!
//! `fwd` implements the `.ts`'s LAZY pipe-through forwarding: `0` while
//! nobody has piped this stream (chunks queue normally); once `pipeThrough`
//! wires a destination, every later `enqueue` forwards straight into that
//! `WritableStream` instead of queueing (`writable::write_into`).

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::{with_rtse, with_rtse_mut};

#[rtse::class("ReadableStreamDefaultController")]
#[derive(Clone, Default)]
pub struct ReadableStreamDefaultController {
    pub(crate) queue: Vec<u64>,
    pub(crate) closed: bool,
    /// `0` = not piped yet; else a `WritableStream` handle to forward into.
    pub(crate) fwd: Handle,
}

#[rtse::class("ReadableStreamDefaultController")]
impl ReadableStreamDefaultController {
    #[rtse::ctor]
    fn new() -> Self {
        Self::default()
    }

    #[rtse::method(name = "enqueue")]
    fn enqueue(self: &mut ReadableStreamDefaultController, chunk: Poly) {
        if self.fwd != 0 {
            super::writable::write_into(self.fwd, chunk);
            return;
        }
        self.queue.push(chunk);
    }

    #[rtse::method(name = "close")]
    fn close(self: &mut ReadableStreamDefaultController) {
        self.closed = true;
        if self.fwd != 0 {
            super::writable::close_of(self.fwd);
        }
    }

    #[rtse::method(name = "error")]
    fn error(self: &mut ReadableStreamDefaultController, _e: Poly) {
        self.closed = true;
    }

    #[rtse::getter]
    fn desired_size(self: &ReadableStreamDefaultController) -> f64 {
        1.0
    }
}

/// Push a chunk straight into `ctl`'s queue (or forward it), bypassing JS
/// dispatch — used by `readable::start`/`writable::write` (transform mode)
/// where the controller handle is already known Rust-side.
pub(crate) fn enqueue_into(ctl_h: u64, chunk: u64) {
    if ctl_h == 0 {
        return;
    }
    let fwd = with_rtse::<ReadableStreamDefaultController, _>(ctl_h, |c| c.map(|c| c.fwd)).unwrap_or(0);
    if fwd != 0 {
        super::writable::write_into(fwd, chunk);
        return;
    }
    with_rtse_mut::<ReadableStreamDefaultController, _>(ctl_h, |c| {
        if let Some(c) = c {
            c.queue.push(chunk);
        }
    });
}

/// Close `ctl` (and forward the close), bypassing JS dispatch.
pub(crate) fn close_of(ctl_h: u64) {
    if ctl_h == 0 {
        return;
    }
    let fwd = with_rtse_mut::<ReadableStreamDefaultController, _>(ctl_h, |c| {
        if let Some(c) = c {
            c.closed = true;
            c.fwd
        } else {
            0
        }
    });
    if fwd != 0 {
        super::writable::close_of(fwd);
    }
}

/// Pop the next queued chunk (FIFO), or `None` if empty — `reader.read()`.
pub(crate) fn shift(ctl_h: u64) -> Option<u64> {
    with_rtse_mut::<ReadableStreamDefaultController, _>(ctl_h, |c| {
        let c = c?;
        if c.queue.is_empty() { None } else { Some(c.queue.remove(0)) }
    })
}

/// `src.pipeThrough(dest)` wiring: drain what `ctl` already queued into
/// `sink_h` (a `WritableStream`), then arm lazy forwarding for everything
/// enqueued from now on. If `ctl` was already closed, close `sink_h` too.
pub(crate) fn pipe_into(ctl_h: u64, sink_h: u64) {
    if ctl_h == 0 || sink_h == 0 {
        return;
    }
    let (drained, closed) = with_rtse_mut::<ReadableStreamDefaultController, _>(ctl_h, |c| {
        let Some(c) = c else { return (Vec::new(), false) };
        let q = std::mem::take(&mut c.queue);
        c.fwd = sink_h;
        (q, c.closed)
    });
    for chunk in drained {
        super::writable::write_into(sink_h, chunk);
    }
    if closed {
        super::writable::close_of(sink_h);
    }
}

/// Is `ctl_h`'s stream closed? (used by `flush`/finalization checks).
#[allow(dead_code)]
pub(crate) fn is_closed(ctl_h: u64) -> bool {
    with_rtse::<ReadableStreamDefaultController, _>(ctl_h, |c| c.map(|c| c.closed)).unwrap_or(true)
}
