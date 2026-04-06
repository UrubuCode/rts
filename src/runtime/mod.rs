pub mod bootstrap;
pub(crate) mod bootstrap_lang;
pub(crate) mod bootstrap_utils;
pub mod bundle;
pub(crate) mod namespaces;
pub mod runner;
pub mod runtime_object;
pub mod state;

use std::collections::BTreeSet;

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
    match name {
        "rts" => Some(BuiltinModule::new("rts", RTS_EXPORTS.iter().copied())),
        _ => None,
    }
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
    "i8",
    "u8",
    "i16",
    "u16",
    "i32",
    "u32",
    "i64",
    "u64",
    "isize",
    "usize",
    "f32",
    "f64",
    "bool",
    "str",
    "WritableStream",
    "ReadableStream",
    "FileHandle",
    "fs",
    "io",
    "process",
    "crypto",
    "global",
    "buffer",
    "promise",
    "task",
];

const COMPILER_DEPENDENCIES: &[&str] = &[
    "anyhow",
    "object",
    "serde",
    "serde_json",
    "ureq",
    "rayon",
    "sha2",
];

const RTS_PENDING_APIS: &[&str] = &[
    "FFI ABI stable layer (C-compatible calls and symbol loader)",
    "Expandir namespaces sem aumentar API plana no modulo `rts`",
    "Process spawn + piping API (stdin/stdout/stderr + exit status)",
    "Async runtime primitives (timers, poller, task scheduler)",
    "Memory safety contract for alloc/dealloc in userland packages",
    "Binary package format for precompiled RTS modules",
    "Cross-platform path API package (normalize/join/resolve)",
    "Networking primitives (TCP, UDP, DNS, HTTP client/server)",
    "Structured diagnostics protocol and source maps for AOT binaries",
    "Package publish/install workflow for ~/.rts/modules registry layout",
];
