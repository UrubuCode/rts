use crate::runtime::bootstrap_lang::JsValue;
use crate::runtime::state as runtime_state;

use super::io;
use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_value,
    decode_hex_payload, hex_encode,
};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "read_to_string",
        callee: "fs.read_to_string",
    },
    NamespaceMember {
        name: "read",
        callee: "fs.read",
    },
    NamespaceMember {
        name: "write",
        callee: "fs.write",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "fs",
    members: MEMBERS,
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "fs.read_to_string" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let result = match runtime_state::fs_read_to_string(&path) {
                Ok(content) => io::result_ok(JsValue::String(content)),
                Err(error) => io::result_err(&error),
            };
            Some(DispatchOutcome::Value(result))
        }
        "fs.read" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let result = match runtime_state::fs_read(&path) {
                Ok(bytes) => io::result_ok(JsValue::String(format!("hex:{}", hex_encode(&bytes)))),
                Err(error) => io::result_err(&error),
            };
            Some(DispatchOutcome::Value(result))
        }
        "fs.write" if args.len() >= 2 => {
            let path = arg_to_string(args, 0);
            let raw_data = arg_to_value(args, 1).to_js_string();
            let payload =
                decode_hex_payload(&raw_data).unwrap_or_else(|| raw_data.as_bytes().to_vec());
            let result = match runtime_state::fs_write(&path, &payload) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(error) => io::result_err(&error),
            };
            Some(DispatchOutcome::Value(result))
        }
        _ => None,
    }
}
