pub mod bootstrap;
pub mod bundle;
pub mod intrinsics;
pub mod runner;

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
    "Process",
    "process",
    "print",
    "panic",
    "clockNow",
    "alloc",
    "dealloc",
];
