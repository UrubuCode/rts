use crate::namespaces::lang::JsValue;
use crate::namespaces::state::{self as runtime_state, AsyncTask};

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u64};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "sleep",
        callee: "task.sleep",
        doc: "Spawns an async sleep task resolved as a promise handle.",
        ts_signature: "sleep(ms: f64, value?: str): promise.Handle",
    },
    NamespaceMember {
        name: "hash_sha256",
        callee: "task.hash_sha256",
        doc: "Spawns an async SHA-256 task resolved as a promise handle.",
        ts_signature: "hash_sha256(data: str): promise.Handle",
    },
    NamespaceMember {
        name: "read_text_file",
        callee: "task.read_text_file",
        doc: "Spawns async text file read task.",
        ts_signature: "read_text_file(path: str): promise.Handle",
    },
    NamespaceMember {
        name: "write_text_file",
        callee: "task.write_text_file",
        doc: "Spawns async text file write task.",
        ts_signature: "write_text_file(path: str, content: str): promise.Handle",
    },
    NamespaceMember {
        name: "append_text_file",
        callee: "task.append_text_file",
        doc: "Spawns async text file append task.",
        ts_signature: "append_text_file(path: str, content: str): promise.Handle",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "task",
    doc: "Async task scheduler helpers that resolve into promise handles.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "task.sleep" if !args.is_empty() => {
            let millis = arg_to_u64(args, 0);
            let value = if args.len() > 1 {
                arg_to_string(args, 1)
            } else {
                format!("slept:{millis}")
            };

            Some(DispatchOutcome::Value(JsValue::Number(
                runtime_state::promise_spawn(AsyncTask::Sleep { millis, value }) as f64,
            )))
        }
        "task.hash_sha256" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::promise_spawn(AsyncTask::HashSha256 {
                data: arg_to_string(args, 0),
            }) as f64,
        ))),
        "task.read_text_file" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::promise_spawn(AsyncTask::ReadTextFile {
                path: arg_to_string(args, 0),
            }) as f64,
        ))),
        "task.write_text_file" if args.len() >= 2 => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::promise_spawn(AsyncTask::WriteTextFile {
                path: arg_to_string(args, 0),
                content: arg_to_string(args, 1),
            }) as f64,
        ))),
        "task.append_text_file" if args.len() >= 2 => Some(DispatchOutcome::Value(
            JsValue::Number(runtime_state::promise_spawn(AsyncTask::AppendTextFile {
                path: arg_to_string(args, 0),
                content: arg_to_string(args, 1),
            }) as f64),
        )),
        _ => None,
    }
}
