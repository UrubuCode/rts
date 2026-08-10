//! Queued records become events, on the thread running JavaScript.
//!
//! The rule every threaded module here pays: a reader thread never calls a
//! listener, because a value belongs to the region of the thread that made it
//! and an `extern "C"` frame cannot unwind past the panic that follows. So this
//! is the only place in `node:http2` that touches the engine on behalf of a
//! socket.
//!
//! The table's lock is released before anything is called. A listener answering
//! a request from inside `'stream'` is the ordinary case, and it calls straight
//! back into `registry`.

use rts_core::entry::{self, Provided};

use super::registry::{self, Queued};

/// The methods a stream delivered by the server side carries.
const STREAM_METHODS: &[(&str, Provided)] = &[
    ("respond", super::js::stream_respond),
    ("write", super::js::stream_write),
    ("end", super::js::stream_end),
    ("close", super::js::stream_close),
];

/// Delivers everything queued for this thread's sessions.
pub(super) fn pump() {
    let mine = std::thread::current().id();
    let due: Vec<(u64, Vec<Queued>)> = registry::with_sessions(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| entry.owner == mine && !entry.queue.is_empty())
            .map(|(&id, entry)| (id, entry.queue.drain(..).collect()))
            .collect()
    });
    for (id, records) in due {
        for record in records {
            deliver(id, record);
        }
    }
}

fn deliver(id: u64, record: Queued) {
    match record {
        Queued::Accepted(session) => {
            let server = registry::with_sessions(|table| table.get(&id).map(|e| e.instance));
            let Some(server) = server else { return };
            // The session gets a JS object now, on this thread — the accept
            // thread could not have made one, because a cell belongs to the
            // region of the thread that allocates it.
            let instance = entry::with_runtime(|context| {
                let prototype = super::js::session_prototype(context);
                let made = entry::make_instance(context, prototype);
                let listeners = entry::make_object(context);
                entry::put_member(context, made, "__events__", listeners);
                let held = entry::make_number(session as f64);
                entry::put_member(context, made, "__sessionId", held);
                made
            });
            registry::with_sessions(|table| {
                if let Some(entry) = table.get_mut(&session) {
                    entry.instance = instance;
                }
            });
            emit1(server, "session", instance);
        }
        Queued::Headers { stream_id, fields, end_stream } => {
            let (session_instance, known) = registry::with_sessions(|table| {
                let entry = table.get(&id);
                (
                    entry.map(|entry| entry.instance),
                    entry.and_then(|entry| entry.streams.get(&stream_id).copied()),
                )
            });
            let Some(session_instance) = session_instance else {
                return;
            };
            let headers = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                for (name, value) in &fields {
                    let held = entry::make_string(context, value);
                    entry::put_member(context, object, name, held);
                }
                object
            });
            match known {
                // A stream this side opened: these are its response headers.
                Some(stream) => {
                    emit1(stream, "response", headers);
                    if end_stream {
                        emit0(stream, "end");
                        forget(id, stream_id);
                    }
                }
                // A stream the peer opened: a request, and the object for it is
                // made here.
                None => {
                    let stream = entry::with_runtime(|context| {
                        let parent = entry::make_prototype(context, "EventEmitter", &[]);
                        let prototype =
                            entry::make_prototype(context, "Http2Stream", STREAM_METHODS);
                        entry::set_prototype_in(context, prototype, parent);
                        let made = entry::make_instance(context, prototype);
                        let listeners = entry::make_object(context);
                        entry::put_member(context, made, "__events__", listeners);
                        let session = entry::make_number(id as f64);
                        entry::put_member(context, made, "__sessionId", session);
                        let held = entry::make_number(f64::from(stream_id));
                        entry::put_member(context, made, "__streamId", held);
                        entry::put_member(context, made, "id", held);
                        made
                    });
                    registry::with_sessions(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.streams.insert(stream_id, stream);
                        }
                    });
                    emit2(session_instance, "stream", stream, headers);
                    if end_stream {
                        emit0(stream, "end");
                    }
                }
            }
        }
        Queued::Data { stream_id, bytes, end_stream } => {
            let stream = registry::with_sessions(|table| {
                table
                    .get(&id)
                    .and_then(|entry| entry.streams.get(&stream_id).copied())
            });
            let Some(stream) = stream else { return };
            let chunk = entry::with_runtime(|context| entry::make_buffer(context, &bytes));
            emit1(stream, "data", chunk);
            if end_stream {
                emit0(stream, "end");
                forget(id, stream_id);
            }
        }
        Queued::Reset { stream_id, error_code } => {
            let stream = registry::with_sessions(|table| {
                table
                    .get(&id)
                    .and_then(|entry| entry.streams.get(&stream_id).copied())
            });
            let Some(stream) = stream else { return };
            let code = entry::make_number(f64::from(error_code));
            emit1(stream, "close", code);
            forget(id, stream_id);
        }
        Queued::Goaway { error_code } => {
            let instance = registry::with_sessions(|table| table.get(&id).map(|e| e.instance));
            let Some(instance) = instance else { return };
            let code = entry::make_number(f64::from(error_code));
            emit1(instance, "goaway", code);
        }
        Queued::Closed(reason) => {
            let instance = registry::with_sessions(|table| {
                if let Some(entry) = table.get_mut(&id) {
                    entry.closed = true;
                }
                table.get(&id).map(|entry| entry.instance)
            });
            let Some(instance) = instance else { return };
            match reason {
                Some(reason) => {
                    let error = entry::with_runtime(|context| {
                        let object = entry::make_object(context);
                        let message = entry::make_string(context, &reason);
                        entry::put_member(context, object, "message", message);
                        object
                    });
                    emit1(instance, "error", error);
                }
                None => emit0(instance, "close"),
            }
        }
    }
}

/// Calls `instance.emit(event, …)`, looked up fresh and never under a borrow —
/// the recipe `dgram` and `net` already use, for the same reason: the listener
/// is user code and it will call back in.
fn emit2(instance: u64, event: &str, a: u64, b: u64) {
    let emitter = entry::with_runtime(|context| entry::get_member(context, instance, "emit"));
    let absent = entry::undefined_value();
    if emitter == absent {
        return;
    }
    let name = entry::with_runtime(|context| entry::make_string(context, event));
    entry::call(emitter, instance, name, a, b, absent);
}

fn emit1(instance: u64, event: &str, value: u64) {
    emit2(instance, event, value, entry::undefined_value());
}

fn emit0(instance: u64, event: &str) {
    let absent = entry::undefined_value();
    emit2(instance, event, absent, absent);
}

/// Forgets a stream once it can carry nothing more.
///
/// Not bookkeeping: `js::source` reports a session with a live stream as
/// outstanding work so the loop keeps turning until a response arrives, and a
/// stream that is never forgotten makes that true forever. The first version
/// left them in and the end-to-end fixture hung — which is the better of the two
/// ways to find out, the other being a program that simply never exits.
fn forget(session: u64, stream_id: u32) {
    registry::with_sessions(|table| {
        if let Some(entry) = table.get_mut(&session) {
            entry.streams.remove(&stream_id);
        }
    });
}
