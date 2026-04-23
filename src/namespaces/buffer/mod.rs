//! `buffer` namespace — heap-owned byte buffers acessados por handle `u64`.
//!
//! Expoe alloc/free/read/write e conversoes UTF-8. Estado compartilhado via
//! `OnceLock<Arc<Mutex<BufferState>>>`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::value::RuntimeValue;

use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u8, arg_to_u64,
    arg_to_usize, arg_to_usize_or_default,
};

// ── Estado de buffers ──────────────────────────────────────────────────────────

#[derive(Default)]
struct BufferState {
    buffers: BTreeMap<u64, Vec<u8>>,
    next_id: u64,
}

static BUFFER_STATE: OnceLock<Arc<Mutex<BufferState>>> = OnceLock::new();

fn state() -> Arc<Mutex<BufferState>> {
    BUFFER_STATE
        .get_or_init(|| Arc::new(Mutex::new(BufferState::default())))
        .clone()
}

fn buffer_alloc(size: usize) -> u64 {
    let s = state();
    let mut s = s.lock().unwrap();
    s.next_id = s.next_id.saturating_add(1);
    let id = s.next_id;
    s.buffers.insert(id, vec![0u8; size]);
    id
}

fn buffer_free(id: u64) -> bool {
    state().lock().unwrap().buffers.remove(&id).is_some()
}

fn buffer_len(id: u64) -> Option<usize> {
    state().lock().unwrap().buffers.get(&id).map(Vec::len)
}

fn buffer_read_u8(id: u64, offset: usize) -> Option<u8> {
    state()
        .lock()
        .unwrap()
        .buffers
        .get(&id)
        .and_then(|b| b.get(offset).copied())
}

fn buffer_write_u8(id: u64, offset: usize, value: u8) -> bool {
    let s = state();
    let mut s = s.lock().unwrap();
    let Some(buf) = s.buffers.get_mut(&id) else {
        return false;
    };
    if offset >= buf.len() {
        return false;
    }
    buf[offset] = value;
    true
}

fn buffer_fill(id: u64, value: u8) -> bool {
    let s = state();
    let mut s = s.lock().unwrap();
    let Some(buf) = s.buffers.get_mut(&id) else {
        return false;
    };
    buf.fill(value);
    true
}

fn buffer_write_text(id: u64, offset: usize, text: &str) -> Option<usize> {
    let s = state();
    let mut s = s.lock().unwrap();
    let buf = s.buffers.get_mut(&id)?;
    let bytes = text.as_bytes();
    let end = offset.saturating_add(bytes.len());
    if end > buf.len() {
        buf.resize(end, 0);
    }
    buf[offset..end].copy_from_slice(bytes);
    Some(bytes.len())
}

fn buffer_read_text(id: u64, offset: usize, length: usize) -> Option<String> {
    let s = state();
    let s = s.lock().unwrap();
    let buf = s.buffers.get(&id)?;
    if offset > buf.len() {
        return None;
    }
    let end = offset.saturating_add(length).min(buf.len());
    Some(String::from_utf8_lossy(&buf[offset..end]).into_owned())
}

fn buffer_copy(
    src_id: u64,
    dst_id: u64,
    src_offset: usize,
    dst_offset: usize,
    length: usize,
) -> Option<usize> {
    let s = state();
    let mut s = s.lock().unwrap();
    let src = s.buffers.get(&src_id)?;
    if src_offset > src.len() {
        return None;
    }
    let src_end = src_offset.saturating_add(length).min(src.len());
    let payload = src[src_offset..src_end].to_vec();
    let dst = s.buffers.get_mut(&dst_id)?;
    let dst_end = dst_offset.saturating_add(payload.len());
    if dst_end > dst.len() {
        dst.resize(dst_end, 0);
    }
    dst[dst_offset..dst_end].copy_from_slice(&payload);
    Some(payload.len())
}

// ── Namespace ──────────────────────────────────────────────────────────────────

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "alloc",
        callee: "buffer.alloc",
        doc: "Allocates a runtime buffer and returns its handle.",
        ts_signature: "alloc(size: usize): Handle",
    },
    NamespaceMember {
        name: "free",
        callee: "buffer.free",
        doc: "Releases a runtime buffer handle.",
        ts_signature: "free(handle: Handle): bool",
    },
    NamespaceMember {
        name: "len",
        callee: "buffer.len",
        doc: "Returns current buffer length.",
        ts_signature: "len(handle: Handle): usize | undefined",
    },
    NamespaceMember {
        name: "read_u8",
        callee: "buffer.read_u8",
        doc: "Reads an unsigned byte from offset.",
        ts_signature: "read_u8(handle: Handle, offset: usize): u8 | undefined",
    },
    NamespaceMember {
        name: "write_u8",
        callee: "buffer.write_u8",
        doc: "Writes an unsigned byte at offset.",
        ts_signature: "write_u8(handle: Handle, offset: usize, value: u8): bool",
    },
    NamespaceMember {
        name: "fill",
        callee: "buffer.fill",
        doc: "Fills entire buffer with a byte value.",
        ts_signature: "fill(handle: Handle, value: u8): bool",
    },
    NamespaceMember {
        name: "write_text",
        callee: "buffer.write_text",
        doc: "Writes UTF-8 text into a buffer from optional offset.",
        ts_signature: "write_text(handle: Handle, content: str, offset?: usize): usize | undefined",
    },
    NamespaceMember {
        name: "read_text",
        callee: "buffer.read_text",
        doc: "Reads UTF-8 text from buffer range.",
        ts_signature: "read_text(handle: Handle, offset: usize, length?: usize): str | undefined",
    },
    NamespaceMember {
        name: "copy",
        callee: "buffer.copy",
        doc: "Copies bytes between two runtime buffers.",
        ts_signature: "copy(source: Handle, target: Handle, sourceOffset?: usize, targetOffset?: usize, length?: usize): usize | undefined",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "buffer",
    doc: "Low-level byte buffer API with explicit handles.",
    members: MEMBERS,
    ts_prelude: &["export type Handle = usize;"],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "buffer.alloc" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Number(
            buffer_alloc(arg_to_usize(args, 0)) as f64,
        ))),
        "buffer.free" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(
            buffer_free(arg_to_u64(args, 0)),
        ))),
        "buffer.len" if !args.is_empty() => Some(DispatchOutcome::Value(
            buffer_len(arg_to_u64(args, 0))
                .map(|value| RuntimeValue::Number(value as f64))
                .unwrap_or(RuntimeValue::Undefined),
        )),
        "buffer.read_u8" if args.len() >= 2 => Some(DispatchOutcome::Value(
            buffer_read_u8(arg_to_u64(args, 0), arg_to_usize(args, 1))
                .map(|value| RuntimeValue::Number(value as f64))
                .unwrap_or(RuntimeValue::Undefined),
        )),
        "buffer.write_u8" if args.len() >= 3 => {
            Some(DispatchOutcome::Value(RuntimeValue::Bool(buffer_write_u8(
                arg_to_u64(args, 0),
                arg_to_usize(args, 1),
                arg_to_u8(args, 2),
            ))))
        }
        "buffer.fill" if args.len() >= 2 => Some(DispatchOutcome::Value(RuntimeValue::Bool(
            buffer_fill(arg_to_u64(args, 0), arg_to_u8(args, 1)),
        ))),
        "buffer.write_text" if args.len() >= 2 => Some(DispatchOutcome::Value(
            buffer_write_text(
                arg_to_u64(args, 0),
                arg_to_usize_or_default(args, 2, 0),
                &arg_to_string(args, 1),
            )
            .map(|written| RuntimeValue::Number(written as f64))
            .unwrap_or(RuntimeValue::Undefined),
        )),
        "buffer.read_text" if args.len() >= 2 => {
            let id = arg_to_u64(args, 0);
            let offset = arg_to_usize(args, 1);
            let requested = arg_to_usize_or_default(args, 2, buffer_len(id).unwrap_or(0));

            Some(DispatchOutcome::Value(
                buffer_read_text(id, offset, requested)
                    .map(RuntimeValue::String)
                    .unwrap_or(RuntimeValue::Undefined),
            ))
        }
        "buffer.copy" if args.len() >= 2 => {
            let src = arg_to_u64(args, 0);
            let dst = arg_to_u64(args, 1);
            let src_offset = arg_to_usize_or_default(args, 2, 0);
            let dst_offset = arg_to_usize_or_default(args, 3, 0);
            let length = arg_to_usize_or_default(args, 4, buffer_len(src).unwrap_or(0));

            Some(DispatchOutcome::Value(
                buffer_copy(src, dst, src_offset, dst_offset, length)
                    .map(|copied| RuntimeValue::Number(copied as f64))
                    .unwrap_or(RuntimeValue::Undefined),
            ))
        }
        _ => None,
    }
}
