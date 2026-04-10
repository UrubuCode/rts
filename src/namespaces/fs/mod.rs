use crate::namespaces::value::JsValue;

use super::io;
use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_value,
    decode_hex_payload, hex_encode,
};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "read_to_string",
        callee: "fs.read_to_string",
        doc: "Reads an UTF-8 file and returns io.Result<string>.",
        ts_signature: "read_to_string<P extends str>(path: P): io.Result<str>",
    },
    NamespaceMember {
        name: "read",
        callee: "fs.read",
        doc: "Reads a file as bytes encoded as a hex payload string in io.Result.",
        ts_signature: "read<P extends str>(path: P): io.Result<str>",
    },
    NamespaceMember {
        name: "write",
        callee: "fs.write",
        doc: "Writes text or hex payload bytes to a file path.",
        ts_signature: "write<P extends str>(path: P, data: str): io.Result<void>",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "fs",
    doc: "Filesystem operations backed by std::fs.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "fs.read_to_string" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let result = match std::fs::read_to_string(&path) {
                Ok(content) => io::result_ok(JsValue::String(content)),
                Err(e) => io::result_err(&format!(
                    "fs::read_to_string('{}') failed: {e}",
                    path.replace('\\', "/")
                )),
            };
            Some(DispatchOutcome::Value(result))
        }
        "fs.read" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let result = match std::fs::read(&path) {
                Ok(bytes) => io::result_ok(JsValue::String(format!("hex:{}", hex_encode(&bytes)))),
                Err(e) => io::result_err(&format!(
                    "fs::read('{}') failed: {e}",
                    path.replace('\\', "/")
                )),
            };
            Some(DispatchOutcome::Value(result))
        }
        "fs.write" if args.len() >= 2 => {
            let path = arg_to_string(args, 0);
            let raw_data = arg_to_value(args, 1).to_js_string();
            let payload =
                decode_hex_payload(&raw_data).unwrap_or_else(|| raw_data.as_bytes().to_vec());
            let result = match std::fs::write(&path, &payload) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&format!(
                    "fs::write('{}') failed: {e}",
                    path.replace('\\', "/")
                )),
            };
            Some(DispatchOutcome::Value(result))
        }
        _ => None,
    }
}
