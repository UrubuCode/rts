use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::bootstrap_lang::JsValue;

pub mod buffer;
pub mod crypto;
pub mod fs;
pub mod global;
pub mod io;
pub mod process;
pub mod promise;
pub mod task;

#[derive(Debug, Clone, Copy)]
pub struct NamespaceMember {
    pub name: &'static str,
    pub callee: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct NamespaceSpec {
    pub name: &'static str,
    pub members: &'static [NamespaceMember],
}

const SPECS: &[NamespaceSpec] = &[
    io::SPEC,
    fs::SPEC,
    process::SPEC,
    crypto::SPEC,
    global::SPEC,
    buffer::SPEC,
    promise::SPEC,
    task::SPEC,
];

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
            let namespace_used = source_list.iter().any(|source| {
                source.contains(spec.name) || source.contains(&format!("{}.", spec.name))
            });

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
        let Some((root, _)) = callee.split_once('.') else {
            return false;
        };
        SPECS.iter().any(|spec| spec.name == root)
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
        .or_else(|| process::dispatch(callee, args))
        .or_else(|| crypto::dispatch(callee, args))
        .or_else(|| global::dispatch(callee, args))
        .or_else(|| buffer::dispatch(callee, args))
        .or_else(|| promise::dispatch(callee, args))
        .or_else(|| task::dispatch(callee, args))
}

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
