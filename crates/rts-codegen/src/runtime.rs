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
    if let Some(ns_name) = name.strip_prefix("node:") {
        if let Some(spec) = crate::nodespace::NODE_SPECS
            .iter()
            .copied()
            .find(|s| s.node_module == ns_name)
        {
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

pub fn builtin_module_keys() -> Vec<&'static str> {
    let mut keys = vec!["rts", "rts:test"];
    for spec in SPECS {
        keys.push(spec.name);
    }
    for spec in crate::nodespace::NODE_SPECS {
        keys.push(spec.ns_prefix);
    }
    keys
}

pub fn rts_exports() -> &'static [&'static str] {
    RTS_EXPORTS
}

const RTS_EXPORTS: &[&str] = &[
    "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "isize", "usize", "f32", "f64",
    "bool", "str", "fs", "io", "math", "bigfloat", "time", "env", "path", "buffer", "string",
    "process", "os", "collections", "hash", "fmt", "crypto", "ui",
];
