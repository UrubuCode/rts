use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::namespaces::lang::JsValue;

pub mod abi;
pub mod buffer;
pub mod crypto;
pub mod fs;
pub mod gc;
pub mod global;
pub mod io;
pub(crate) mod lang;
pub mod net;
pub mod process;
pub mod promise;
pub mod rust;
pub(crate) mod state;
pub mod task;

#[derive(Debug, Clone, Copy)]
pub struct NamespaceMember {
    pub name: &'static str,
    pub callee: &'static str,
    pub doc: &'static str,
    pub ts_signature: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct NamespaceSpec {
    pub name: &'static str,
    pub doc: &'static str,
    pub members: &'static [NamespaceMember],
    pub ts_prelude: &'static [&'static str],
}

const SPECS: &[NamespaceSpec] = &[
    io::SPEC,
    fs::SPEC,
    net::SPEC,
    process::SPEC,
    crypto::SPEC,
    global::SPEC,
    buffer::SPEC,
    promise::SPEC,
    task::SPEC,
    gc::SPEC,
    rust::SPEC,
];

#[derive(Debug, Clone)]
pub struct NamespaceCatalogEntry {
    pub namespace: String,
    pub callees: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NamespaceDocEntry {
    pub namespace: String,
    pub doc: String,
    pub functions: Vec<NamespaceFunctionDocEntry>,
}

#[derive(Debug, Clone)]
pub struct NamespaceFunctionDocEntry {
    pub name: String,
    pub callee: String,
    pub doc: String,
    pub ts_signature: String,
}

pub fn catalog() -> Vec<NamespaceCatalogEntry> {
    SPECS
        .iter()
        .map(|spec| NamespaceCatalogEntry {
            namespace: spec.name.to_string(),
            callees: spec
                .members
                .iter()
                .map(|member| member.callee.to_string())
                .collect(),
        })
        .collect()
}

pub fn documentation_catalog() -> Vec<NamespaceDocEntry> {
    SPECS
        .iter()
        .map(|spec| NamespaceDocEntry {
            namespace: spec.name.to_string(),
            doc: spec.doc.to_string(),
            functions: spec
                .members
                .iter()
                .map(|member| NamespaceFunctionDocEntry {
                    name: member.name.to_string(),
                    callee: member.callee.to_string(),
                    doc: member.doc.to_string(),
                    ts_signature: member.ts_signature.to_string(),
                })
                .collect(),
        })
        .collect()
}

pub fn namespace_for_callee(callee: &str) -> Option<&'static str> {
    let (root, _) = callee.split_once('.')?;
    SPECS
        .iter()
        .find(|spec| spec.name == root)
        .map(|spec| spec.name)
}

pub fn is_catalog_callee(callee: &str) -> bool {
    SPECS
        .iter()
        .flat_map(|spec| spec.members.iter())
        .any(|member| member.callee == callee)
}

#[derive(Debug, Clone)]
pub enum DispatchOutcome {
    Value(JsValue),
    Emit(String),
    Panic(String),
}

#[derive(Debug, Clone, Default)]
pub struct NamespaceUsage {
    namespaces: BTreeSet<String>,
    functions: BTreeSet<String>,
}

impl NamespaceUsage {
    pub fn from_sources<'a>(sources: impl IntoIterator<Item = &'a str>) -> Self {
        let source_list = sources.into_iter().collect::<Vec<_>>();
        let mut usage = Self::default();

        for spec in SPECS {
            let namespace_used = source_list
                .iter()
                .any(|source| source.contains(&format!("{}.", spec.name)));

            if namespace_used {
                usage.namespaces.insert(spec.name.to_string());
            }

            for member in spec.members {
                if source_list
                    .iter()
                    .any(|source| source.contains(member.callee))
                {
                    usage.namespaces.insert(spec.name.to_string());
                    usage.functions.insert(member.callee.to_string());
                }
            }
        }

        usage
    }

    pub fn is_namespace_enabled(&self, namespace: &str) -> bool {
        self.namespaces.contains(namespace)
    }

    pub fn is_function_enabled(&self, callee: &str) -> bool {
        self.functions.contains(callee)
    }

    pub fn is_builtin_callee(&self, callee: &str) -> bool {
        namespace_for_callee(callee).is_some()
    }

    pub fn enabled_functions(&self) -> impl Iterator<Item = &str> {
        self.functions.iter().map(String::as_str)
    }

    pub fn enabled_namespaces(&self) -> impl Iterator<Item = &str> {
        self.namespaces.iter().map(String::as_str)
    }
}

pub fn namespace_object(name: &str, usage: &NamespaceUsage) -> Option<JsValue> {
    let spec = SPECS.iter().find(|spec| spec.name == name)?;
    if !usage.is_namespace_enabled(name) {
        return None;
    }

    let mut map = BTreeMap::new();
    for member in spec.members {
        if usage.is_function_enabled(member.callee) {
            map.insert(
                member.name.to_string(),
                JsValue::NativeFunction(member.callee.to_string()),
            );
        }
    }

    if map.is_empty() {
        return None;
    }

    Some(JsValue::Object(map))
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    io::dispatch(callee, args)
        .or_else(|| fs::dispatch(callee, args))
        .or_else(|| net::dispatch(callee, args))
        .or_else(|| process::dispatch(callee, args))
        .or_else(|| crypto::dispatch(callee, args))
        .or_else(|| global::dispatch(callee, args))
        .or_else(|| buffer::dispatch(callee, args))
        .or_else(|| promise::dispatch(callee, args))
        .or_else(|| task::dispatch(callee, args))
        .or_else(|| gc::dispatch(callee, args))
        .or_else(|| rust::dispatch(callee, args))
}

pub fn default_typescript_output_path() -> PathBuf {
    PathBuf::from("packages").join("rts-types").join("rts.d.ts")
}

pub fn emit_typescript_declarations(output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let content = render_typescript_declarations();
    std::fs::write(output_path, content)
        .with_context(|| format!("failed to write {}", output_path.display()))
}

fn render_typescript_declarations() -> String {
    let mut out = String::new();
    out.push_str("declare module \"rts\" {\n");
    out.push_str(RTS_BASE_TYPES);
    out.push('\n');

    for spec in SPECS {
        write_doc_block(&mut out, 2, spec.doc);
        out.push_str(&format!("  export namespace {} {{\n", spec.name));

        for block in spec.ts_prelude {
            for line in block.lines() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }

        for member in spec.members {
            write_doc_block(&mut out, 4, member.doc);
            out.push_str("    export function ");
            out.push_str(member.ts_signature);
            out.push_str(";\n");
        }

        out.push_str("  }\n\n");
    }

    out.push_str("}\n");
    out
}

fn write_doc_block(out: &mut String, indent: usize, doc: &str) {
    let padding = " ".repeat(indent);
    out.push_str(&padding);
    out.push_str("/**\n");
    for line in doc.lines() {
        out.push_str(&padding);
        out.push_str(" * ");
        out.push_str(line.trim());
        out.push('\n');
    }
    out.push_str(&padding);
    out.push_str(" */\n");
}

const RTS_BASE_TYPES: &str = r#"  export type i8 = number;
  export type u8 = number;
  export type i16 = number;
  export type u16 = number;
  export type i32 = number;
  export type u32 = number;
  export type i64 = number;
  export type u64 = number;
  export type isize = number;
  export type usize = number;
  export type f32 = number;
  export type f64 = number;
  export type bool = boolean;
  export type str = string;

  export interface WritableStream {
    write(message: str): void;
  }

  export interface ReadableStream {
    read(maxBytes?: usize): str;
  }

  export interface FileHandle {
    close(): void;
  }"#;

pub(crate) fn arg_to_string(args: &[JsValue], index: usize) -> String {
    args.get(index)
        .cloned()
        .unwrap_or(JsValue::Undefined)
        .to_js_string()
}

pub(crate) fn arg_to_value(args: &[JsValue], index: usize) -> JsValue {
    args.get(index).cloned().unwrap_or(JsValue::Undefined)
}

pub(crate) fn arg_to_usize(args: &[JsValue], index: usize) -> usize {
    arg_to_usize_or_default(args, index, 0)
}

pub(crate) fn arg_to_usize_or_default(args: &[JsValue], index: usize, default: usize) -> usize {
    let value = args
        .get(index)
        .cloned()
        .unwrap_or(JsValue::Number(default as f64))
        .to_number();

    if value.is_nan() || value.is_sign_negative() {
        return default;
    }

    value as usize
}

pub(crate) fn arg_to_u64(args: &[JsValue], index: usize) -> u64 {
    arg_to_usize(args, index) as u64
}

pub(crate) fn arg_to_u8(args: &[JsValue], index: usize) -> u8 {
    let value = arg_to_usize(args, index).min(u8::MAX as usize);
    value as u8
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(nibble_to_hex(byte >> 4));
        output.push(nibble_to_hex(byte & 0x0f));
    }
    output
}

pub(crate) fn decode_hex_payload(value: &str) -> Option<Vec<u8>> {
    let hex = value.strip_prefix("hex:")?;
    if hex.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let raw = hex.as_bytes();
    let mut index = 0usize;

    while index < raw.len() {
        let high = hex_to_nibble(raw[index])?;
        let low = hex_to_nibble(raw[index + 1])?;
        bytes.push((high << 4) | low);
        index += 2;
    }

    Some(bytes)
}

pub(crate) fn current_time_millis() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

fn hex_to_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{NamespaceUsage, catalog, namespace_for_callee, render_typescript_declarations};

    #[test]
    fn usage_ignores_import_only_namespace_reference() {
        let source = r#"import { io, process } from "rts"; const value = 1;"#;
        let usage = NamespaceUsage::from_sources(std::iter::once(source));
        assert_eq!(usage.enabled_namespaces().count(), 0);
        assert_eq!(usage.enabled_functions().count(), 0);
    }

    #[test]
    fn usage_enables_called_runtime_function() {
        let source = r#"import { io } from "rts"; io.print("ok");"#;
        let usage = NamespaceUsage::from_sources(std::iter::once(source));
        assert!(usage.is_namespace_enabled("io"));
        assert!(usage.is_function_enabled("io.print"));
    }

    #[test]
    fn catalog_resolves_builtin_callee_namespace() {
        let entries = catalog();
        assert!(entries.iter().any(|entry| {
            entry.namespace == "io" && entry.callees.iter().any(|callee| callee == "io.print")
        }));
        assert_eq!(namespace_for_callee("io.print"), Some("io"));
    }

    #[test]
    fn typescript_declarations_include_comments() {
        let dts = render_typescript_declarations();
        assert!(dts.contains("declare module \"rts\""));
        assert!(dts.contains("Writes a message to stdout."));
        assert!(dts.contains("export function print(message: str): void;"));
    }
}
