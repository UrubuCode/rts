//! Byte-view formatting for `util.inspect`.

use rts_core::entry;

use super::inspect::{self, Options};
use super::values::{get, number_of, own_key_strings, string_of};

/// Format a Buffer or Uint8Array, or leave other objects to the structural walk.
pub(super) fn format(value: u64, options: Options, seen: &mut Vec<u64>) -> Option<String> {
    let name = constructor_name(value)?;
    match name.as_str() {
        "Buffer" => format_buffer(value, options, seen),
        "Uint8Array" => format_uint8_array(value),
        _ => None,
    }
}

/// Node's `<Buffer …>` representation, including user-defined properties.
fn format_buffer(value: u64, options: Options, seen: &mut Vec<u64>) -> Option<String> {
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, value))?;
    if seen.contains(&value) {
        return Some("[Circular]".to_owned());
    }
    seen.push(value);
    let limit = inspect_max_bytes();
    let shown = bytes.len().min(limit);
    let mut parts: Vec<String> = bytes[..shown]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if bytes.len() > shown {
        parts.push(format!("... {} more bytes", bytes.len() - shown));
    }
    let mut rendered = format!("<Buffer {}", parts.join(" "));
    let extras = own_key_strings(value)
        .into_iter()
        .filter(|key| !internal_key(key) && key.parse::<usize>().is_err())
        .map(|key| {
            format!(
                "{}: {}",
                inspect::key_text(&key),
                inspect::format_value(get(value, &key), options.inner(), seen)
            )
        })
        .collect::<Vec<_>>();
    if !extras.is_empty() {
        if !parts.is_empty() {
            rendered.push_str(", ");
        }
        rendered.push_str(&extras.join(", "));
    }
    rendered.push('>');
    seen.pop();
    Some(rendered)
}

/// Node's typed-array spelling for the Uint8Array values used as Buffer extras.
fn format_uint8_array(value: u64) -> Option<String> {
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, value))?;
    let parts = bytes
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>();
    Some(format!(
        "Uint8Array({}) {}",
        bytes.len(),
        inspect::wrapped('[', ']', &parts)
    ))
}

/// Read the mutable limit exposed by `node:buffer`.
fn inspect_max_bytes() -> usize {
    let namespace = entry::with_runtime(|context| entry::module_at_name(context, "node:buffer"));
    match number_of(get(namespace, "INSPECT_MAX_BYTES")) {
        Some(value) if value.is_infinite() && value.is_sign_positive() => usize::MAX,
        Some(value) if value.is_finite() && value >= 0.0 => value as usize,
        _ => 50,
    }
}

/// Metadata installed on every Buffer view, not user-defined properties.
fn internal_key(key: &str) -> bool {
    matches!(
        key,
        "byteLength" | "byteOffset" | "length" | "buffer" | "parent"
    )
}

fn constructor_name(value: u64) -> Option<String> {
    string_of(get(get(value, "constructor"), "name"))
}
