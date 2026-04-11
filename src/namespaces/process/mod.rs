use crate::namespaces::value::RuntimeValue;

use super::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u64,
    arg_to_usize_or_default, current_time_millis,
};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "args",
        callee: "process.args",
        doc: "Returns process CLI arguments.",
        ts_signature: "args(): globalThis.Array<str> | str",
    },
    NamespaceMember {
        name: "cwd",
        callee: "process.cwd",
        doc: "Returns current working directory.",
        ts_signature: "cwd(): str",
    },
    NamespaceMember {
        name: "chdir",
        callee: "process.chdir",
        doc: "Changes process working directory.",
        ts_signature: "chdir(path: str): void",
    },
    NamespaceMember {
        name: "env_get",
        callee: "process.env_get",
        doc: "Reads an environment variable.",
        ts_signature: "env_get(name: str): str | undefined",
    },
    NamespaceMember {
        name: "env_set",
        callee: "process.env_set",
        doc: "Sets an environment variable.",
        ts_signature: "env_set(name: str, value: str): void",
    },
    NamespaceMember {
        name: "platform",
        callee: "process.platform",
        doc: "Returns target OS name.",
        ts_signature: "platform(): str",
    },
    NamespaceMember {
        name: "arch",
        callee: "process.arch",
        doc: "Returns target architecture.",
        ts_signature: "arch(): str",
    },
    NamespaceMember {
        name: "pid",
        callee: "process.pid",
        doc: "Returns current process id.",
        ts_signature: "pid(): i32",
    },
    NamespaceMember {
        name: "sleep",
        callee: "process.sleep",
        doc: "Sleeps current thread for milliseconds.",
        ts_signature: "sleep(ms: f64): void",
    },
    NamespaceMember {
        name: "exit",
        callee: "process.exit",
        doc: "Aborts execution with an exit code signal.",
        ts_signature: "exit(code?: i32): never",
    },
    NamespaceMember {
        name: "clock_now",
        callee: "process.clock_now",
        doc: "Returns wall clock time in milliseconds.",
        ts_signature: "clock_now(): f64",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "process",
    doc: "Process utilities such as env, cwd, pid and time.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "process.arch" if args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            std::env::consts::ARCH.to_string(),
        ))),
        "process.platform" if args.is_empty() => Some(DispatchOutcome::Value(
            RuntimeValue::String(std::env::consts::OS.to_string()),
        )),
        "process.pid" if args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Number(
            std::process::id() as f64,
        ))),
        "process.cwd" if args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            std::env::current_dir()
                .ok()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ))),
        "process.chdir" if !args.is_empty() => {
            let path = arg_to_string(args, 0);
            let _ = std::env::set_current_dir(path);
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "process.env_get" if !args.is_empty() => Some(DispatchOutcome::Value(
            std::env::var(arg_to_string(args, 0))
                .map(RuntimeValue::String)
                .unwrap_or(RuntimeValue::Undefined),
        )),
        "process.env_set" if args.len() >= 2 => {
            let key = arg_to_string(args, 0);
            let value = arg_to_string(args, 1);
            // SAFETY: runtime mutation of process environment is expected behavior for RTS.
            unsafe { std::env::set_var(key, value) };
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "process.clock_now" if args.is_empty() => Some(DispatchOutcome::Value(
            RuntimeValue::Number(current_time_millis() as f64),
        )),
        "process.args" if args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            std::env::args().skip(1).collect::<Vec<_>>().join(","),
        ))),
        "process.sleep" if !args.is_empty() => {
            std::thread::sleep(std::time::Duration::from_millis(arg_to_u64(args, 0)));
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "process.exit" => Some(DispatchOutcome::Panic(format!(
            "process exited with code {}",
            arg_to_usize_or_default(args, 0, 0)
        ))),
        _ => None,
    }
}
