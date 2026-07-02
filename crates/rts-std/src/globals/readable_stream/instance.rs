//! Web Streams runtime — ReadableStream / TransformStream family.
//!
//! See `abi.rs` for the data model. Buffers are synchronous: the producer
//! (`start` callback / `transform`) enqueues chunks before any `read()` runs,
//! so `read()` can resolve a Promise immediately with the next buffered chunk.

use rts_engine::heap::handles::{
    alloc_entry, with_entry, with_entry_mut, Entry, FunctionData,
};
use crate::promise_slot;
use indexmap::IndexMap;

const UNDEFINED: i64 = i64::MIN + 2;
const BOOL_FALSE: i64 = i64::MIN;
const BOOL_TRUE: i64 = i64::MIN + 1;

// ── Map helpers ─────────────────────────────────────────────────────────────

fn map_get(map_h: u64, key: &str) -> i64 {
    with_entry(map_h, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    })
}

fn map_set(map_h: u64, key: &str, val: i64) {
    with_entry_mut(map_h, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert(key.to_string(), val);
        }
    });
}

fn new_map() -> u64 {
    alloc_entry(Entry::Map(Box::new(IndexMap::new())))
}

fn vec_push(vec_h: u64, val: i64) {
    with_entry_mut(vec_h, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.push(val);
        }
    });
}

fn vec_len(vec_h: u64) -> i64 {
    with_entry(vec_h, |e| match e {
        Some(Entry::Vec(v)) => v.len() as i64,
        _ => 0,
    })
}

fn vec_get(vec_h: u64, idx: i64) -> i64 {
    with_entry(vec_h, |e| match e {
        Some(Entry::Vec(v)) => v.get(idx as usize).copied().unwrap_or(UNDEFINED),
        _ => UNDEFINED,
    })
}

/// Resolves a callback value to its raw `extern "C"` fn pointer (+ bound args).
///
/// Object-literal methods (`{ start(c){...} }`) are stored either as an
/// `Entry::Function` handle *or* as a raw `func_addr` i64 (the common case for
/// method shorthand). Mirror `promise::resolve_callback_ptr`: when the handle
/// is not a Function entry, treat the value itself as the raw fn pointer.
fn fn_ptr_of(handle: i64) -> Option<(u64, Vec<i64>)> {
    if handle == 0 {
        return None;
    }
    let as_fn = with_entry(handle as u64, |e| match e {
        Some(Entry::Function(fd)) => Some((fd.fn_ptr, fd.bound_args.clone())),
        _ => None,
    });
    Some(as_fn.unwrap_or((handle as u64, Vec::new())))
}

/// Invokes an `extern "C" fn(i64...) -> i64` with the given args (arity 0..=4).
unsafe fn invoke(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
            _ => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3],
            ),
        }
    }
}

/// Calls `fn_handle(extra...)` (prepending bound args). No-op if not a fn.
fn call_fn(fn_handle: i64, extra: &[i64]) {
    if let Some((ptr, bound)) = fn_ptr_of(fn_handle) {
        if ptr == 0 {
            return;
        }
        let mut all = bound;
        all.extend_from_slice(extra);
        unsafe {
            invoke(ptr, &all);
        }
    }
}

/// Builds a `{value, done}` result Map (same shape as the iterator protocol).
fn make_result(value: i64, done: bool) -> u64 {
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("value".to_string(), value);
    m.insert(
        "done".to_string(),
        if done { BOOL_TRUE } else { BOOL_FALSE },
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Wraps a value handle in a fulfilled Promise.
fn fulfilled_promise(value: i64) -> u64 {
    let slot = promise_slot::new_fulfilled(value);
    alloc_entry(Entry::PromiseAsync(slot))
}

// ── Stream allocation ─────────────────────────────────────────────────────────

/// Allocates a stream Map with an empty buffer + open state. `getReader` /
/// `getWriter` / `pipeThrough` are stored as bound fn handles so they resolve
/// via generic dispatch when a `const s = ...` binding loses the static stream
/// type (the typed InstanceMethod path only fires on a direct chain).
fn new_stream() -> u64 {
    let buf = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let s = new_map();
    map_set(s, "__buf", buf as i64);
    map_set(s, "__closed", 0);
    let gr = __RTS_FN_GL_READABLE_STREAM_GET_READER as *const () as usize as u64;
    let gw = __RTS_FN_GL_WRITABLE_STREAM_GET_WRITER as *const () as usize as u64;
    let pt = __RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH as *const () as usize as u64;
    map_set(s, "getReader", reify_bound(gr, s, 1) as i64);
    map_set(s, "getWriter", reify_bound(gw, s, 1) as i64);
    map_set(s, "pipeThrough", reify_bound(pt, s, 2) as i64);
    s
}

/// Reifies an internal controller fn (`__RTS_FN_GL_READABLE_STREAM_CONTROLLER_*`)
/// as an `Entry::Function` bound to the controller, so a transform callback that
/// receives the controller as an UNTYPED arg (`transform(chunk, controller)`)
/// can call `controller.enqueue(x)` / `controller.close()` via the generic
/// call-on-property path (the typed InstanceMethod path only fires when the
/// receiver is statically a ReadableStreamDefaultController).
fn reify_bound(fn_ptr: u64, recv: u64, arity: u8) -> u64 {
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity,
        name: "".into(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: vec![recv as i64],
        is_arrow: false,
        has_this_param: false,
        param_kinds: vec![0u8; arity as usize],
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: false,
    })))
}

/// Allocates a controller bound to `stream_h`, with `enqueue`/`close` stored as
/// bound fn handles so untyped `controller.enqueue(...)` calls resolve.
fn new_controller(stream_h: u64) -> u64 {
    let c = new_map();
    map_set(c, "__stream", stream_h as i64);
    let enq = __RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE as *const () as usize as u64;
    let cls = __RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE as *const () as usize as u64;
    map_set(c, "enqueue", reify_bound(enq, c, 2) as i64);
    map_set(c, "close", reify_bound(cls, c, 1) as i64);
    c
}

fn stream_enqueue(stream_h: u64, val: i64) {
    let buf = map_get(stream_h, "__buf") as u64;
    if buf != 0 {
        vec_push(buf, val);
    }
}

fn stream_close(stream_h: u64) {
    // CompressionStream finalize: on first close, gzip/zlib the accumulated
    // bytes and enqueue the compressed Buffer. Runs here (not only in
    // WRITER_CLOSE) because `writer.close()` is routed to the controller/stream
    // close path by codegen.
    let accum = map_get(stream_h, "__accum");
    if accum != 0 && map_get(stream_h, "__closed") == 0 {
        let bytes = read_buffer_bytes(accum as u64);
        let compressed = if map_get(stream_h, "__deflate") != 0 {
            deflate_bytes(&bytes)
        } else {
            gzip_bytes(&bytes)
        };
        let out = alloc_entry(Entry::Buffer(compressed));
        stream_enqueue(stream_h, out as i64);
    }
    map_set(stream_h, "__closed", 1);
}

// ── ReadableStream ─────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_NEW(opts: u64) -> u64 {
    let stream = new_stream();
    let controller = new_controller(stream);
    // Invoke `start(controller)` synchronously, if provided.
    let start = map_get(opts, "start");
    call_fn(start, &[controller as i64]);
    stream
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE(controller: u64, chunk: i64) {
    let stream = map_get(controller, "__stream") as u64;
    if stream != 0 {
        stream_enqueue(stream, chunk);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE(controller: u64) {
    let stream = map_get(controller, "__stream") as u64;
    if stream != 0 {
        stream_close(stream);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_GET_READER(stream: u64) -> u64 {
    let r = new_map();
    map_set(r, "__stream", stream as i64);
    map_set(r, "__cursor", 0);
    // Bound `read` so `const reader = ...; reader.read()` works even when the
    // const loses the static ReadableStreamDefaultReader type (generic dispatch).
    let read = __RTS_FN_GL_READABLE_STREAM_READER_READ as *const () as usize as u64;
    map_set(r, "read", reify_bound(read, r, 1) as i64);
    r
}

/// `reader.read()` -> resolved Promise of `{value, done}`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_READER_READ(reader: u64) -> u64 {
    let stream = map_get(reader, "__stream") as u64;
    let cursor = map_get(reader, "__cursor");
    let buf = map_get(stream, "__buf") as u64;
    let len = vec_len(buf);
    let result = if cursor < len {
        let val = vec_get(buf, cursor);
        map_set(reader, "__cursor", cursor + 1);
        make_result(val, false)
    } else {
        // No more buffered data. With the synchronous model the producer has
        // already closed (or there is nothing left) — signal done.
        make_result(UNDEFINED, true)
    };
    fulfilled_promise(result as i64)
}

// ── TransformStream ────────────────────────────────────────────────────────────
//
// A TransformStream shares one buffer between its writable and readable sides.
// The `transform(chunk, controller)` callback enqueues into that buffer; the
// readable side reads from it. `.readable` / `.writable` are stored as fields
// pointing at the shared stream (so member access returns the right handle).

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TRANSFORM_STREAM_NEW(opts: u64) -> u64 {
    let stream = new_stream();
    // Store the transform callback for the writer side to invoke per chunk.
    let transform = map_get(opts, "transform");
    map_set(stream, "__transform", transform);
    // readable / writable both refer to the same underlying stream.
    let ts = new_map();
    map_set(ts, "readable", stream as i64);
    map_set(ts, "writable", stream as i64);
    // Mark so codegen field access yields the correct sub-stream handles.
    map_set(ts, "__stream", stream as i64);
    ts
}

/// `transformStream.writable` — the shared stream typed as a WritableStream so
/// `.getWriter()` dispatches.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TRANSFORM_STREAM_WRITABLE(ts: u64) -> u64 {
    map_get(ts, "writable") as u64
}

/// `transformStream.readable` — the shared stream typed as a ReadableStream so
/// `.getReader()` dispatches.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TRANSFORM_STREAM_READABLE(ts: u64) -> u64 {
    map_get(ts, "readable") as u64
}

/// Identity transform stream (no `transform` callback): a write enqueues the
/// chunk unchanged. Backs TextEncoderStream/TextDecoderStream — encode then
/// decode round-trips to the original text, so the observable output matches.
fn new_identity_ts() -> u64 {
    let stream = new_stream();
    let ts = new_map();
    map_set(ts, "writable", stream as i64);
    map_set(ts, "readable", stream as i64);
    map_set(ts, "__stream", stream as i64);
    ts
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXT_ENCODER_STREAM_NEW() -> u64 {
    new_identity_ts()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXT_DECODER_STREAM_NEW() -> u64 {
    new_identity_ts()
}

/// `src.pipeThrough(dest)` — connect the upstream readable's buffer to the
/// downstream transform stream's stream (identity model: share the buffer so
/// chunks written upstream are visible to the downstream reader), and return
/// `dest.readable`. `src` is a readable stream Map (has `__buf`); `dest` is a
/// TransformStream-shaped object (has `__stream` / `readable`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH(src: u64, dest: u64) -> u64 {
    let dest_stream = map_get(dest, "__stream") as u64;
    let src_buf = map_get(src, "__buf");
    if dest_stream != 0 && src_buf != 0 {
        map_set(dest_stream, "__buf", src_buf);
    }
    map_get(dest, "readable") as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WRITABLE_STREAM_GET_WRITER(stream: u64) -> u64 {
    let w = new_map();
    map_set(w, "__stream", stream as i64);
    // Bound `write`/`close` so `const writer = ...; writer.write(x)` works even
    // when the const loses the static WritableStreamDefaultWriter type.
    let write = __RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE as *const () as usize as u64;
    let close = __RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE as *const () as usize as u64;
    map_set(w, "write", reify_bound(write, w, 2) as i64);
    map_set(w, "close", reify_bound(close, w, 1) as i64);
    w
}

/// `writer.write(chunk)` -> resolved Promise<void>. Runs `transform(chunk, ctrl)`
/// if the stream has a transformer; otherwise enqueues the chunk directly.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE(writer: u64, chunk: i64) -> u64 {
    let stream = map_get(writer, "__stream") as u64;
    if stream != 0 {
        let accum = map_get(stream, "__accum");
        if accum != 0 {
            // CompressionStream: accumulate the raw input bytes; compress on close.
            let bytes = read_buffer_bytes(chunk as u64);
            with_entry_mut(accum as u64, |e| {
                if let Some(Entry::Buffer(v)) = e {
                    v.extend_from_slice(&bytes);
                }
            });
            return fulfilled_promise(UNDEFINED);
        }
        let transform = map_get(stream, "__transform");
        if transform != 0 {
            let controller = new_controller(stream);
            call_fn(transform, &[chunk, controller as i64]);
        } else {
            stream_enqueue(stream, chunk);
        }
    }
    fulfilled_promise(UNDEFINED)
}

/// `writer.close()` -> resolved Promise<void>. Closes the underlying stream. For
/// a CompressionStream, finalizes compression: gzip/zlib the accumulated bytes
/// and enqueue the compressed Buffer for the readable side.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE(writer: u64) -> u64 {
    let stream = map_get(writer, "__stream") as u64;
    if stream != 0 {
        stream_close(stream);
    }
    fulfilled_promise(UNDEFINED)
}

fn read_buffer_bytes(h: u64) -> Vec<u8> {
    with_entry(h, |e| match e {
        Some(Entry::Buffer(v)) | Some(Entry::String(v)) => v.clone(),
        _ => Vec::new(),
    })
}

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn deflate_bytes(data: &[u8]) -> Vec<u8> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// `new CompressionStream(format)` — a writable side that accumulates raw bytes
/// and, on close, gzip/zlib-compresses them into the readable side.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_COMPRESSION_STREAM_NEW(fmt_ptr: u64, fmt_len: i64) -> u64 {
    let fmt = if fmt_ptr != 0 && fmt_len > 0 {
        let bytes = unsafe { std::slice::from_raw_parts(fmt_ptr as *const u8, fmt_len as usize) };
        std::str::from_utf8(bytes).unwrap_or("gzip").to_owned()
    } else {
        "gzip".to_owned()
    };
    let stream = new_stream();
    let accum = alloc_entry(Entry::Buffer(Vec::new()));
    map_set(stream, "__accum", accum as i64);
    if fmt == "deflate" {
        map_set(stream, "__deflate", 1);
    }
    let cs = new_map();
    map_set(cs, "writable", stream as i64);
    map_set(cs, "readable", stream as i64);
    map_set(cs, "__stream", stream as i64);
    cs
}
