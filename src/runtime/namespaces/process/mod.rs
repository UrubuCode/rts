use crate::runtime::bootstrap_lang::JsValue;

use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u64,
    arg_to_usize_or_default, current_time_millis,
};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "args",
        callee: "process.args",
    },
    NamespaceMember {
        name: "cwd",
        callee: "process.cwd",
    },
    NamespaceMember {
        name: "chdir",
        callee: "process.chdir",
    },
    NamespaceMember {
        name: "env_get",
        callee: "process.env_get",
    },
    NamespaceMember {
        name: "env_set",
        callee: "process.env_set",
    },
    NamespaceMember {
        name: "platform",
        callee: "process.platform",
    },
    NamespaceMember {
        name: "arch",
        callee: "process.arch",
    },
    NamespaceMember {
        name: "pid",
        callee: "process.pid",
    },
    NamespaceMember {
        name: "sleep",
        callee: "process.sleep",
    },
    NamespaceMember {
        name: "exit",
        callee: "process.exit",
    },
    NamespaceMember {
        name: "clock_now",
        callee: "process.clock_now",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "process",
    members: MEMBERS,
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "process.arch" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            std::env::consts::ARCH.to_string(),
        ))),
        "process.platform" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            std::env::consts::OS.to_string(),
        ))),
        "process.pid" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            std::process::id() as f64,
        ))),
        "process.cwd" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            std::env::current_dir()
                .ok()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ))),
        "process.chdir" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let _ = std::env::set_current_dir(path);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "process.env_get" if !args.is_empty() => Some(DispatchOutcome::Value(
            std::env::var(arg_to_string(args, 0))
                .map(JsValue::String)
                .unwrap_or(JsValue::Undefined),
        )),
        "process.env_set" if args.len() >= 2 => {
            let key = arg_to_string(args, 0);
            let value = arg_to_string(args, 1);
            // SAFETY: runtime mutation of process environment is expected behavior for RTS.
            unsafe { std::env::set_var(key, value) };
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "process.clock_now" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            current_time_millis() as f64,
        ))),
        "process.args" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            std::env::args().skip(1).collect::<Vec<_>>().join(","),
        ))),
        "process.sleep" if !args.is_empty() => {
            std::thread::sleep(std::time::Duration::from_millis(arg_to_u64(args, 0)));
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "process.exit" => Some(DispatchOutcome::Panic(format!(
            "process exited with code {}",
            arg_to_usize_or_default(args, 0, 0)
        ))),
        _ => None,
    }
}
