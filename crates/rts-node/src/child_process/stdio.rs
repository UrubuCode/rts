//! `child.stdout`, `child.stderr`, `child.stdin` — the piped half of a spawn.
//!
//! # Why this was absent, and what actually made it possible
//!
//! [`super::spawn_async`]'s doc refused it: *"a real `Readable`/`Writable`
//! facade needs the same listener-delivery problem this module already has,
//! times three concurrent streams"*. The delivery problem is the one
//! `entry::loops` solved for everybody: a source registers a `fn`, the host
//! pumps it on the program's own thread, and a background thread never touches
//! a JS value. This module has nothing new to invent — it queues bytes on the
//! table `spawn_async` already keeps and lets that module's `pump` turn them
//! into events, which is the same path `'exit'` already travels.
//!
//! So what was missing was not a mechanism. It was three objects.
//!
//! # What it cost to be absent
//!
//! Measured 2026-08-24 against Node's own suite: `Cannot read properties of
//! undefined (reading 'on')` was the single most frequent message in the whole
//! corpus after the internals, and `child.stdout` was where most of it came
//! from — `spawn()` answered a process whose streams were `null`, so the first
//! line after it died. Node's default for `spawn` is `'pipe'`, not inherit,
//! which the old code got backwards and said so.
//!
//! # What these objects are, and what they are NOT
//!
//! They are `EventEmitter`s that emit `'data'`, `'end'` and `'close'`, plus the
//! handful of methods a program calls on them without thinking (`setEncoding`,
//! `pause`, `resume`, `destroy`, `pipe`). They are **not** `stream.Readable`
//! instances: `readable instanceof require('stream').Readable` is false here,
//! and there is no backpressure — `pause()` records the flag and the reader
//! thread keeps reading, because the OS pipe is what would apply the back
//! pressure and this side has already taken the bytes off it.
//!
//! That is named rather than approximated. What a program gets is the event
//! stream, which is what `child.stdout.on('data', …)` is written for; what it
//! does not get is a `Readable` to compose with, and asking for one fails at
//! the `instanceof` rather than halfway through a pipeline.

use std::io::{Read, Write};
use std::process::{ChildStderr, ChildStdin, ChildStdout};

use rts_core::entry::{self, Provided};

use super::shared::string;

/// Which of a child's streams a queued chunk belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Fd {
    /// The child's standard output.
    Out,
    /// The child's standard error.
    Err,
}

impl Fd {
    /// The member of the child object this stream is published as.
    pub(super) fn member(self) -> &'static str {
        match self {
            Fd::Out => "stdout",
            Fd::Err => "stderr",
        }
    }
}

/// What a readable side can do.
///
/// `pipe` is here because a program writes `child.stdout.pipe(process.stdout)`
/// as one line and expects output; the rest are the calls that appear before
/// anyone has decided whether the stream is being consumed.
const READABLE: &[(&str, Provided)] = &[
    ("setEncoding", set_encoding),
    ("pause", pause),
    ("resume", resume),
    ("destroy", destroy),
    ("pipe", pipe),
    ("read", read_nothing),
];

/// What the writable side can do.
const WRITABLE: &[(&str, Provided)] = &[
    ("write", write),
    ("end", end),
    ("destroy", destroy),
    ("cork", noop_self),
    ("uncork", noop_self),
];

/// Builds `child.stdout` / `child.stderr`.
///
/// The `EventEmitter` prototype is reached the same way `spawn_async` reaches
/// it for the child itself — one prototype per name, so `on`/`once`/`emit` are
/// the real ones and not a second implementation living on this object.
pub(super) fn readable(context: &mut entry::Context, id: u64, fd: Fd) -> u64 {
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "ChildProcessStream", READABLE);
    entry::set_prototype_in(context, prototype, event_emitter);
    let instance = entry::make_instance(context, prototype);
    // Both properties, not `__events__` alone: `events.rs`'s
    // `eventNames()`/no-argument `removeAllListeners()` read
    // `__eventNames__` specifically, and a missing one reads as an
    // always-empty list forever, silently.
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
    let event_names = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__eventNames__", event_names);
    let held = entry::make_number(id as f64);
    entry::put_member(context, instance, "__procId", held);
    let held = entry::make_string(context, fd.member());
    entry::put_member(context, instance, "__fd", held);
    // Present and `null` until `setEncoding` names one, which is what Node's
    // `readableEncoding` answers — the difference between "bytes" and "text" is
    // a question a program asks this object.
    let held = entry::null_in(context);
    entry::put_member(context, instance, "__encoding", held);
    let held = entry::boolean_value(false);
    entry::put_member(context, instance, "destroyed", held);
    // Which direction this end of the pipe goes. A program asks — Node's own
    // suite asserts `stdin.writable === true` and `stdin.readable === false`
    // on the line before it writes — and an absent flag reads `undefined`,
    // which is neither true nor false and fails a strict comparison against
    // both.
    let yes = entry::boolean_value(true);
    entry::put_member(context, instance, "readable", yes);
    let no = entry::boolean_value(false);
    entry::put_member(context, instance, "writable", no);
    instance
}

/// Builds `child.stdin`.
pub(super) fn writable(context: &mut entry::Context, id: u64) -> u64 {
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "ChildProcessStdin", WRITABLE);
    entry::set_prototype_in(context, prototype, event_emitter);
    let instance = entry::make_instance(context, prototype);
    // Both properties, not `__events__` alone: `events.rs`'s
    // `eventNames()`/no-argument `removeAllListeners()` read
    // `__eventNames__` specifically, and a missing one reads as an
    // always-empty list forever, silently.
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
    let event_names = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__eventNames__", event_names);
    let held = entry::make_number(id as f64);
    entry::put_member(context, instance, "__procId", held);
    let held = entry::boolean_value(false);
    entry::put_member(context, instance, "destroyed", held);
    let yes = entry::boolean_value(true);
    entry::put_member(context, instance, "writable", yes);
    let no = entry::boolean_value(false);
    entry::put_member(context, instance, "readable", no);
    instance
}

/// Reads one pipe to its end, off the JS thread, queueing what it finds.
///
/// # Why a thread per stream and not one that watches both
///
/// Because a read on a pipe blocks, and a single thread reading `stdout` would
/// not notice `stderr` filling — the child then blocks writing to a pipe nobody
/// drains, which is the classic deadlock `capture.rs` already documents solving
/// the same way for the synchronous side.
pub(super) fn drain<R: Read + Send + 'static>(id: u64, fd: Fd, mut source: R) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match source.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let chunk = buffer[..read].to_vec();
                    if !super::spawn_async::queue_data(id, fd, chunk) {
                        // The process left the table — nobody will ever read
                        // what this queues, so reading on would be work for a
                        // listener that cannot exist.
                        return;
                    }
                }
            }
        }
        super::spawn_async::queue_end(id, fd);
    });
}

/// Starts the two reader threads for a spawned child.
pub(super) fn attach(id: u64, out: Option<ChildStdout>, err: Option<ChildStderr>) {
    if let Some(out) = out {
        drain(id, Fd::Out, out);
    }
    if let Some(err) = err {
        drain(id, Fd::Err, err);
    }
}

/// `stream.setEncoding(enc)` — what a `'data'` chunk is delivered as.
///
/// Node answers the stream itself so the call chains. The encoding is stored on
/// the object rather than in the table because it is a fact about the JS-side
/// view, not about the pipe: two objects over one child (there are two) each
/// decode independently, which is what Node does.
extern "C" fn set_encoding(_e: u64, this: u64, encoding: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = super::shared::text(encoding);
    entry::with_runtime(|context| {
        let held = match &text {
            Some(name) => entry::make_string(context, name),
            None => entry::null_in(context),
        };
        entry::put_member(context, this, "__encoding", held);
    });
    this
}

/// `stream.pause()` / `stream.resume()`.
///
/// The flag is recorded and the reader thread does NOT stop: see this module's
/// header. `resume` is what most programs call after `pause` in the same tick,
/// and both answer the stream so the call chains.
extern "C" fn pause(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    flag(this, "__paused", true);
    this
}

extern "C" fn resume(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    flag(this, "__paused", false);
    this
}

/// `stream.destroy()` — marks it and stops delivery.
///
/// The pipe is not closed here: the reader thread owns the handle and closing
/// it from this side would be a race with a read already in flight. What the
/// flag does is stop the pump emitting, which is what a program that destroys a
/// stream is asking for.
extern "C" fn destroy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    flag(this, "destroyed", true);
    this
}

/// `stream.read()` — always `null`.
///
/// The bytes have already been handed to `'data'` listeners by the time any
/// program could call this, so there is nothing buffered to answer. `null` is
/// what a `Readable` with nothing available answers, which is the honest result
/// rather than an empty buffer that would look like an empty chunk.
extern "C" fn read_nothing(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::null_value()
}

/// `source.pipe(destination)` — records the destination and answers it.
///
/// The pump is what writes: it looks for `__pipe` when it has a chunk. Answering
/// the DESTINATION and not the source is what makes `a.pipe(b).pipe(c)` work,
/// and is the one part of `pipe`'s contract a program depends on structurally.
extern "C" fn pipe(_e: u64, this: u64, destination: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        entry::put_member(context, this, "__pipe", destination);
    });
    destination
}

/// `child.stdin.write(chunk)` — straight to the pipe.
///
/// Answers `true` always, which is what a `Writable` answers when it did not
/// have to buffer. Nothing here buffers, so the answer is never false and a
/// program watching for `'drain'` waits forever — named in the header rather
/// than emitted as an event nothing produced.
extern "C" fn write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = proc_id_of(this) else {
        return entry::boolean_value(false);
    };
    let bytes = match super::shared::text(chunk) {
        Some(text) => text.into_bytes(),
        None => entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default(),
    };
    entry::boolean_value(super::spawn_async::write_stdin(id, &bytes))
}

/// `child.stdin.end(chunk?)` — writes the last chunk and closes the pipe.
///
/// Closing matters: a child reading to end-of-input never returns until the
/// write side is dropped, so a program that only ever called `write` would hang
/// its child rather than see it exit.
extern "C" fn end(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = proc_id_of(this) else {
        return this;
    };
    if chunk != entry::undefined_value() {
        let bytes = match super::shared::text(chunk) {
            Some(text) => text.into_bytes(),
            None => entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default(),
        };
        super::spawn_async::write_stdin(id, &bytes);
    }
    super::spawn_async::close_stdin(id);
    this
}

extern "C" fn noop_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// Writes a boolean member.
fn flag(this: u64, name: &str, held: bool) {
    entry::with_runtime(|context| {
        let value = entry::boolean_value(held);
        entry::put_member(context, this, name, value);
    });
}

/// The process id a stream object belongs to.
fn proc_id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, string("__procId"));
    entry::number_of(value).map(|value| value as u64)
}

/// Writes to a held stdin handle. Kept here because it is the only place that
/// knows a `ChildStdin` is a `Write`.
pub(super) fn write_to(handle: &mut ChildStdin, bytes: &[u8]) -> bool {
    handle.write_all(bytes).is_ok() && handle.flush().is_ok()
}
