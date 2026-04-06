use crate::runtime::bootstrap_lang::JsValue;
use crate::runtime::state as runtime_state;

use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u8, arg_to_u64,
    arg_to_usize, arg_to_usize_or_default,
};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "alloc",
        callee: "buffer.alloc",
    },
    NamespaceMember {
        name: "free",
        callee: "buffer.free",
    },
    NamespaceMember {
        name: "len",
        callee: "buffer.len",
    },
    NamespaceMember {
        name: "read_u8",
        callee: "buffer.read_u8",
    },
    NamespaceMember {
        name: "write_u8",
        callee: "buffer.write_u8",
    },
    NamespaceMember {
        name: "fill",
        callee: "buffer.fill",
    },
    NamespaceMember {
        name: "write_text",
        callee: "buffer.write_text",
    },
    NamespaceMember {
        name: "read_text",
        callee: "buffer.read_text",
    },
    NamespaceMember {
        name: "copy",
        callee: "buffer.copy",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "buffer",
    members: MEMBERS,
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "buffer.alloc" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::buffer_alloc(arg_to_usize(args, 0)) as f64,
        ))),
        "buffer.free" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::buffer_free(arg_to_u64(args, 0)),
        ))),
        "buffer.len" if !args.is_empty() => Some(DispatchOutcome::Value(
            runtime_state::buffer_len(arg_to_u64(args, 0))
                .map(|value| JsValue::Number(value as f64))
                .unwrap_or(JsValue::Undefined),
        )),
        "buffer.read_u8" if args.len() >= 2 => Some(DispatchOutcome::Value(
            runtime_state::buffer_read_u8(arg_to_u64(args, 0), arg_to_usize(args, 1))
                .map(|value| JsValue::Number(value as f64))
                .unwrap_or(JsValue::Undefined),
        )),
        "buffer.write_u8" if args.len() >= 3 => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::buffer_write_u8(
                arg_to_u64(args, 0),
                arg_to_usize(args, 1),
                arg_to_u8(args, 2),
            ),
        ))),
        "buffer.fill" if args.len() >= 2 => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::buffer_fill(arg_to_u64(args, 0), arg_to_u8(args, 1)),
        ))),
        "buffer.write_text" if args.len() >= 2 => Some(DispatchOutcome::Value(
            runtime_state::buffer_write_text(
                arg_to_u64(args, 0),
                arg_to_usize_or_default(args, 2, 0),
                &arg_to_string(args, 1),
            )
            .map(|written| JsValue::Number(written as f64))
            .unwrap_or(JsValue::Undefined),
        )),
        "buffer.read_text" if args.len() >= 2 => {
            let id = arg_to_u64(args, 0);
            let offset = arg_to_usize(args, 1);
            let requested =
                arg_to_usize_or_default(args, 2, runtime_state::buffer_len(id).unwrap_or(0));

            Some(DispatchOutcome::Value(
                runtime_state::buffer_read_text(id, offset, requested)
                    .map(JsValue::String)
                    .unwrap_or(JsValue::Undefined),
            ))
        }
        "buffer.copy" if args.len() >= 2 => {
            let src = arg_to_u64(args, 0);
            let dst = arg_to_u64(args, 1);
            let src_offset = arg_to_usize_or_default(args, 2, 0);
            let dst_offset = arg_to_usize_or_default(args, 3, 0);
            let length =
                arg_to_usize_or_default(args, 4, runtime_state::buffer_len(src).unwrap_or(0));

            Some(DispatchOutcome::Value(
                runtime_state::buffer_copy(src, dst, src_offset, dst_offset, length)
                    .map(|copied| JsValue::Number(copied as f64))
                    .unwrap_or(JsValue::Undefined),
            ))
        }
        _ => None,
    }
}
