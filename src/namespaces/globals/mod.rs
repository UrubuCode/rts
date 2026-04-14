use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_value};
use crate::namespaces::value::RuntimeValue;

pub fn set(key: impl Into<String>, value: RuntimeValue) {
    crate::namespaces::abi::set_runtime_global_property(&key.into(), value);
}

pub fn get(key: &str) -> Option<RuntimeValue> {
    crate::namespaces::abi::get_runtime_global_property(key)
}

pub fn has(key: &str) -> bool {
    crate::namespaces::abi::has_runtime_global_property(key)
}

pub fn delete(key: &str) -> bool {
    crate::namespaces::abi::delete_runtime_global_property(key)
}

pub fn keys_csv() -> String {
    crate::namespaces::abi::runtime_global_keys_csv()
}

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "set",
        callee: "globals.set",
        doc: "Defines a global variable accessible from anywhere in the program.",
        ts_signature: "set(name: str, value: any): void",
    },
    NamespaceMember {
        name: "get",
        callee: "globals.get",
        doc: "Reads a global variable by name.",
        ts_signature: "get(name: str): any",
    },
    NamespaceMember {
        name: "has",
        callee: "globals.has",
        doc: "Returns true when a global variable is defined.",
        ts_signature: "has(name: str): bool",
    },
    NamespaceMember {
        name: "remove",
        callee: "globals.remove",
        doc: "Removes a global variable by name.",
        ts_signature: "remove(name: str): bool",
    },
    NamespaceMember {
        name: "keys",
        callee: "globals.keys",
        doc: "Returns a comma-separated list with every global key.",
        ts_signature: "keys(): str",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "globals",
    doc: "Global key-value registry shared by all modules in the runtime.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "globals.set" if args.len() >= 2 => {
            let key = arg_to_string(args, 0);
            let value = arg_to_value(args, 1);
            set(key, value);
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "globals.get" if !args.is_empty() => {
            let key = arg_to_string(args, 0);
            Some(DispatchOutcome::Value(
                get(&key).unwrap_or(RuntimeValue::Undefined),
            ))
        }
        "globals.has" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(has(
            &arg_to_string(args, 0),
        )))),
        "globals.remove" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(
            delete(&arg_to_string(args, 0)),
        ))),
        "globals.keys" => Some(DispatchOutcome::Value(RuntimeValue::String(keys_csv()))),
        _ => None,
    }
}
