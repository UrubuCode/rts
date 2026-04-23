use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub mod value;
pub use value::RuntimeValue;

pub mod abi;
pub mod buffer;
pub mod crypto;
pub mod fs;
pub mod gc;
pub mod globals;
pub mod io;
pub mod json;
pub mod net;
pub mod process;
pub mod promise;
pub mod rust;
pub mod str;
pub mod task;
pub mod test;

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
    globals::SPEC,
    buffer::SPEC,
    promise::SPEC,
    task::SPEC,
    gc::SPEC,
    test::SPEC,
    json::SPEC,
    str::SPEC,
    rust::SPEC,
    rust::NATIVES_SPEC,
    rust::HOTOPS_SPEC,
    rust::DEBUG_SPEC,
];

/// Namespaces that get a standalone `rts:<name>` module (user-facing).
/// Internal `rts.*` sub-namespaces are excluded here.
const SPLIT_SPECS: &[&NamespaceSpec] = &[
    &io::SPEC,
    &fs::SPEC,
    &net::SPEC,
    &process::SPEC,
    &crypto::SPEC,
    &globals::SPEC,
    &buffer::SPEC,
    &promise::SPEC,
    &task::SPEC,
    &gc::SPEC,
    &test::SPEC,
    &json::SPEC,
    &str::SPEC,
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
    Value(RuntimeValue),
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

pub fn namespace_object(name: &str, usage: &NamespaceUsage) -> Option<RuntimeValue> {
    let spec = SPECS.iter().find(|spec| spec.name == name)?;
    if !usage.is_namespace_enabled(name) {
        return None;
    }

    let mut map = BTreeMap::new();
    for member in spec.members {
        if usage.is_function_enabled(member.callee) {
            map.insert(
                member.name.to_string(),
                RuntimeValue::NativeFunction(member.callee.to_string()),
            );
        }
    }

    if map.is_empty() {
        return None;
    }

    Some(RuntimeValue::Object(map))
}

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    let root = callee.split('.').next()?;
    match root {
        "io" => io::dispatch(callee, args),
        "fs" => fs::dispatch(callee, args),
        "net" => net::dispatch(callee, args),
        "process" => process::dispatch(callee, args),
        "crypto" => crypto::dispatch(callee, args),
        "globals" => globals::dispatch(callee, args),
        "buffer" => buffer::dispatch(callee, args),
        "promise" => promise::dispatch(callee, args),
        "task" => task::dispatch(callee, args),
        "gc" => gc::dispatch(callee, args),
        "test" => test::dispatch(callee, args),
        "JSON" => json::dispatch(callee, args),
        "str" => str::dispatch(callee, args),
        "rust" => rust::dispatch(callee, args),
        _ => None,
    }
}

/// Returns the named exports for a `rts:<name>` builtin module.
/// Includes each function name plus `"default"` (the namespace object).
pub fn namespace_exports_for(name: &str) -> Option<Vec<&'static str>> {
    SPLIT_SPECS
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| {
            let mut exports: Vec<&'static str> = spec.members.iter().map(|m| m.name).collect();
            exports.push("default");
            exports
        })
}

/// Retorna os nomes de todos os namespaces split disponiveis (sem prefixo `rts:`).
/// Usado por diagnosticos para sugerir alternativas.
pub fn namespace_names() -> Vec<&'static str> {
    SPLIT_SPECS.iter().map(|spec| spec.name).collect()
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

/// Emits one `rts:<name>.d.ts` file per user-facing namespace into the given directory.
pub fn emit_split_typescript_declarations(output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    for spec in SPLIT_SPECS {
        let file_name = format!("{}.d.ts", spec.name);
        let path = output_dir.join(&file_name);
        let content = render_namespace_declaration(spec);
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

fn render_namespace_declaration(spec: &NamespaceSpec) -> String {
    // Detecta quais OUTROS namespaces split este spec referencia nas suas
    // signatures (ex: `fs` usa `io.Result<T>`). Para cada um, precisamos
    // re-declarar o `Result<T>` localmente no module bloco, porque
    // `declare module "rts:fs"` nao pode ver tipos de `declare module "rts:io"`
    // a menos que importe. Optamos por inline para manter o .d.ts auto-contido
    // (sem dependencia de ordem de carregamento de declaracoes).
    let referenced_peers = collect_peer_namespace_refs(spec);

    let mut out = String::new();
    out.push_str(&format!("declare module \"rts:{}\" {{\n", spec.name));
    out.push_str(RTS_BASE_TYPES_FLAT);
    out.push('\n');

    // Re-declara preludes dos peers referenciados dentro de um sub-namespace
    // com o mesmo nome (ex: `namespace io { export type Result<T> = ... }`),
    // de forma que `io.Result<T>` resolva localmente.
    for peer_name in &referenced_peers {
        if let Some(peer_spec) = SPLIT_SPECS.iter().find(|s| s.name == *peer_name) {
            if !peer_spec.ts_prelude.is_empty() {
                out.push_str(&format!("  export namespace {peer_name} {{\n"));
                for block in peer_spec.ts_prelude {
                    for line in block.lines() {
                        out.push_str("    ");
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push('\n');
                }
                out.push_str("  }\n\n");
            }
        }
    }

    for block in spec.ts_prelude {
        for line in block.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    for member in spec.members {
        write_doc_block(&mut out, 2, member.doc);
        out.push_str("  export function ");
        out.push_str(member.ts_signature);
        out.push_str(";\n");
    }

    // Default export: namespace object with the same function signatures.
    out.push_str("\n  const _default: {\n");
    for member in spec.members {
        out.push_str("    ");
        out.push_str(member.ts_signature);
        out.push_str(";\n");
    }
    out.push_str("  };\n");
    out.push_str("  export default _default;\n");

    out.push_str("}\n");
    out
}

/// Varre as signatures de um spec procurando por referencias `<peer>.<type>`
/// onde `<peer>` e nome de outro namespace split. Usado pelo emissor de
/// arquivos split para saber quais preludes re-declarar localmente.
fn collect_peer_namespace_refs(spec: &NamespaceSpec) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for member in spec.members {
        for peer in SPLIT_SPECS.iter() {
            if peer.name == spec.name {
                continue;
            }
            let needle = format!("{}.", peer.name);
            if member.ts_signature.contains(&needle) && !found.contains(&peer.name) {
                found.push(peer.name);
            }
        }
    }
    found
}

fn render_typescript_declarations() -> String {
    let mut out = String::new();
    out.push_str("declare module \"rts\" {\n");
    out.push_str(RTS_BASE_TYPES);
    out.push('\n');

    // Namespaces de primeiro nivel (name sem ponto) sao renderizados
    // diretamente. Namespaces com ponto (ex: `rts.natives`, `rts.hotops`)
    // sao aninhados sob um unico `namespace rts { ... }` porque TypeScript
    // nao aceita `namespace rts.natives { ... }` — o ponto no nome nao e
    // sintaxe valida para namespace declaration.
    let mut dotted: std::collections::BTreeMap<&str, Vec<&NamespaceSpec>> =
        std::collections::BTreeMap::new();

    for spec in SPECS {
        if let Some((parent, _leaf)) = spec.name.split_once('.') {
            dotted.entry(parent).or_default().push(spec);
            continue;
        }

        render_flat_namespace_body(&mut out, spec, 2);
    }

    for (parent, specs) in dotted {
        out.push_str(&format!("  export namespace {parent} {{\n"));
        for spec in specs {
            let leaf = spec
                .name
                .split_once('.')
                .map(|(_, l)| l)
                .unwrap_or(spec.name);
            // Criamos uma cópia leve com o nome trocado para o leaf, assim
            // `render_flat_namespace_body` nao precisa saber do agrupamento.
            let leaf_spec = NamespaceSpec {
                name: leaf,
                doc: spec.doc,
                members: spec.members,
                ts_prelude: spec.ts_prelude,
            };
            render_flat_namespace_body(&mut out, &leaf_spec, 4);
        }
        out.push_str("  }\n\n");
    }

    out.push_str("}\n");
    out
}

/// Escreve um `export namespace <name> { ... }` no output com a indentacao
/// informada (em espacos). Usado tanto para namespaces de primeiro nivel
/// (indent = 2) quanto para sub-namespaces aninhados (indent = 4).
fn render_flat_namespace_body(out: &mut String, spec: &NamespaceSpec, indent: usize) {
    let padding = " ".repeat(indent);
    let inner_padding = " ".repeat(indent + 2);

    write_doc_block(out, indent, spec.doc);
    out.push_str(&padding);
    out.push_str(&format!("export namespace {} {{\n", spec.name));

    for block in spec.ts_prelude {
        for line in block.lines() {
            out.push_str(&inner_padding);
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    for member in spec.members {
        write_doc_block(out, indent + 2, member.doc);
        out.push_str(&inner_padding);
        out.push_str("export function ");
        out.push_str(member.ts_signature);
        out.push_str(";\n");
    }

    out.push_str(&padding);
    out.push_str("}\n\n");
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

const RTS_BASE_TYPES_FLAT: &str = r#"  export type i8 = number;
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
  export type str = string;"#;

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

pub(crate) fn arg_to_string(args: &[RuntimeValue], index: usize) -> String {
    args.get(index)
        .cloned()
        .unwrap_or(RuntimeValue::Undefined)
        .to_js_string()
}

pub(crate) fn arg_to_value(args: &[RuntimeValue], index: usize) -> RuntimeValue {
    args.get(index).cloned().unwrap_or(RuntimeValue::Undefined)
}

pub(crate) fn arg_to_usize(args: &[RuntimeValue], index: usize) -> usize {
    arg_to_usize_or_default(args, index, 0)
}

pub(crate) fn arg_to_usize_or_default(
    args: &[RuntimeValue],
    index: usize,
    default: usize,
) -> usize {
    let value = args
        .get(index)
        .cloned()
        .unwrap_or(RuntimeValue::Number(default as f64))
        .to_number();

    if value.is_nan() || value.is_sign_negative() {
        return default;
    }

    value as usize
}

pub(crate) fn arg_to_u64(args: &[RuntimeValue], index: usize) -> u64 {
    arg_to_usize(args, index) as u64
}

pub(crate) fn arg_to_u8(args: &[RuntimeValue], index: usize) -> u8 {
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
