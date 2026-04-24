pub mod bundle;

use std::collections::BTreeSet;

use crate::abi::SPECS;

#[derive(Debug, Clone)]
pub struct BuiltinModule {
    pub name: String,
    pub key: String,
    pub exports: BTreeSet<String>,
}

impl BuiltinModule {
    pub fn new(name: impl Into<String>, exports: impl IntoIterator<Item = &'static str>) -> Self {
        let name = name.into();
        let key = format!("<builtin:{name}>");
        let exports = exports.into_iter().map(ToString::to_string).collect();

        Self { name, key, exports }
    }
}

pub fn builtin_module(name: &str) -> Option<BuiltinModule> {
    if name == "rts" {
        return Some(BuiltinModule::new("rts", RTS_EXPORTS.iter().copied()));
    }

    if let Some(ns_name) = name.strip_prefix("rts:") {
        if let Some(spec) = SPECS.iter().copied().find(|s| s.name == ns_name) {
            let exports: Vec<String> = spec
                .members
                .iter()
                .map(|m| m.name.to_string())
                .chain(std::iter::once("default".to_string()))
                .collect();
            let mut module = BuiltinModule::new(name, std::iter::empty::<&'static str>());
            module.exports = exports.into_iter().collect();
            return Some(module);
        }
    }

    None
}

/// Returns every known builtin module key so diagnostics can suggest
/// corrections ("did you mean 'rts:fs'?").
pub fn builtin_module_keys() -> Vec<&'static str> {
    let mut keys = vec!["rts"];
    for spec in SPECS {
        keys.push(spec.name);
    }
    keys
}

pub fn rts_exports() -> &'static [&'static str] {
    RTS_EXPORTS
}

pub fn compiler_dependencies() -> &'static [&'static str] {
    COMPILER_DEPENDENCIES
}

pub fn rts_pending_apis() -> &'static [&'static str] {
    RTS_PENDING_APIS
}

const RTS_EXPORTS: &[&str] = &[
    "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "isize", "usize", "f32", "f64", "bool",
    "str", "fs", "io", "math", "bigfloat", "time",
];

const COMPILER_DEPENDENCIES: &[&str] = &[
    "anyhow",
    "object",
    "serde",
    "serde_json",
    "ureq",
    "rayon",
    "sha2",
    "cranelift-codegen",
    "cranelift-module",
    "cranelift-object",
    "cranelift-jit",
];

const RTS_PENDING_APIS: &[&str] = &[
    "Codegen rebuild on top of the new ABI surface",
    "AOT pipeline rewire to consume abi::SPECS directly",
    "GC namespace with handle table + string pool",
    "Networking namespace (TCP, UDP, HTTP)",
    "Process namespace (spawn, env, exit, argv)",
    "Crypto namespace (SHA family, HMAC, AEAD)",
    "Async runtime primitives (timers, task scheduler)",
    "Structured diagnostics + source maps for AOT binaries",
    "builtin/ tarball embed + `rts i` installer",
];
