//! `SharedArrayBuffer` — the single-threaded shape.
//!
//! # What is real and what is not
//!
//! This engine is single-threaded today: there is no second OS thread that
//! could hold a `Context`, so there is nothing for these bytes to be shared
//! WITH. What is genuine is everything a program can observe from inside one
//! thread — a `SharedArrayBuffer` is bytes, `Atomics.*` (see [`super::atomics`])
//! reads and writes them as non-concurrent read-modify-write operations, which
//! is semantically identical to what `Atomics` answers in a JS engine that IS
//! multi-threaded, as long as nothing else runs between the read and the write.
//! Nothing else does, here — there is no second thread to interleave with.
//!
//! What is NOT here: `postMessage`-style transfer to a worker, or any other
//! cross-thread visibility, because there is no other thread. A program asking
//! `sab instanceof SharedArrayBuffer` or `sab.byteLength` gets the truth; a
//! program relying on a second agent observing a write gets nothing, because
//! this runtime has no second agent.
//!
//! Built as a thin variant of [`super::array_buffer`]: same byte store, same
//! view machinery, a different constructor and its own prototype so
//! `instanceof` tells the two apart, which is the one thing that has to be
//! true independent of threading.

use super::with_current;
use crate::entry::objects;
use crate::value::Value;

/// `SharedArrayBuffer`.
#[rtse::class("SharedArrayBuffer")]
impl SharedArrayBuffer {
    /// `new SharedArrayBuffer(byteLength)` — bytes, exactly like `ArrayBuffer`;
    /// see the module doc for what "shared" does not mean here.
    #[construct]
    fn build(this: u64, length: f64) -> u64 {
        let count = super::as_count(length);
        with_current(|context| {
            match Value(this).as_slot() {
                Some(cell) => {
                    super::install_bytes(context, cell, count);
                    Value::from_slot(cell).bits()
                }
                None => match super::new_buffer(context, count) {
                    Some(cell) => Value::from_slot(cell).bits(),
                    None => objects::undefined_of(context),
                },
            }
        })
    }

    /// `sab.slice(begin, end)` — a copy, in a new `SharedArrayBuffer`... in a
    /// real engine. Here it answers a copy in a new buffer of the SAME bytes
    /// machinery `ArrayBuffer.prototype.slice` uses, since the two share a
    /// representation and nothing downstream tells them apart by class.
    fn slice(this: u64, begin: u64, end: u64) -> u64 {
        let begin = super::optional_number(begin);
        let end = super::optional_number(end);
        with_current(|context| {
            let absent = objects::undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let Some(bytes) = context.bytes_at(cell) else {
                return absent;
            };
            let (first, last) = super::range(bytes.len(), begin, end);
            let taken = bytes[first..last].to_vec();
            let Some(made) = super::new_buffer(context, taken.len()) else {
                return absent;
            };
            if let Some(destination) = context.bytes_at_mut(made) {
                destination.copy_from_slice(&taken);
            }
            Value::from_slot(made).bits()
        })
    }
}
