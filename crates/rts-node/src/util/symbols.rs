//! node:util — the `#[rtse::function]`-declared free functions. `format` is
//! variadic in JS → fixed-arity overloads (0..4) sharing the JS name `format`;
//! `isDeepStrictEqual` reuses the `assert` deep-equal; the string utilities are
//! pure.

use rts_engine::abi::ty::Poly;

use super::format::{format, format_opts};
use super::inspect::{inspect, inspect_with_options};
use super::words::{error_name, strip_vt, style_text, to_usv_string};

/// `util.format(fmt)`.
#[rtse::function(module = "node:util", value = "format")]
fn format0(fmt: &str) -> String {
    format(fmt, &[])
}
/// `util.format(fmt, a0)`.
#[rtse::function(module = "node:util", value = "format", overload = "a1")]
fn format1(fmt: &str, a0: Poly) -> String {
    format(fmt, &[a0])
}
/// `util.format(fmt, a0, a1)`.
#[rtse::function(module = "node:util", value = "format", overload = "a2")]
fn format2(fmt: &str, a0: Poly, a1: Poly) -> String {
    format(fmt, &[a0, a1])
}
/// `util.format(fmt, a0, a1, a2)`.
#[rtse::function(module = "node:util", value = "format", overload = "a3")]
fn format3(fmt: &str, a0: Poly, a1: Poly, a2: Poly) -> String {
    format(fmt, &[a0, a1, a2])
}
/// `util.format(fmt, a0, a1, a2, a3)`.
#[rtse::function(module = "node:util", value = "format", overload = "a4")]
fn format4(fmt: &str, a0: Poly, a1: Poly, a2: Poly, a3: Poly) -> String {
    format(fmt, &[a0, a1, a2, a3])
}

/// `util.formatWithOptions(inspectOptions, fmt)`.
#[rtse::function(module = "node:util", value = "formatWithOptions")]
fn format_opts0(opts: Poly, fmt: &str) -> String {
    format_opts(fmt, &[], opts)
}
/// `util.formatWithOptions(inspectOptions, fmt, a0)`.
#[rtse::function(module = "node:util", value = "formatWithOptions", overload = "a1")]
fn format_opts1(opts: Poly, fmt: &str, a0: Poly) -> String {
    format_opts(fmt, &[a0], opts)
}
/// `util.formatWithOptions(inspectOptions, fmt, a0, a1)`.
#[rtse::function(module = "node:util", value = "formatWithOptions", overload = "a2")]
fn format_opts2(opts: Poly, fmt: &str, a0: Poly, a1: Poly) -> String {
    format_opts(fmt, &[a0, a1], opts)
}

/// `util.inspect(value)` → its textual representation.
#[rtse::function(module = "node:util", value = "inspect")]
fn inspect_value(value: Poly) -> String {
    inspect(value)
}

/// `util.inspect(value, options)` — honors `options.depth`.
#[rtse::function(module = "node:util", value = "inspect", overload = "opts")]
fn inspect_value_opts(value: Poly, options: Poly) -> String {
    inspect_with_options(value, options)
}

/// `util.isDeepStrictEqual(a, b)`.
#[rtse::function(module = "node:util", value = "isDeepStrictEqual")]
fn is_deep_strict_equal(a: Poly, b: Poly) -> bool {
    crate::assert::words::deep_equal(a, b, true)
}

/// `util.toUSVString(str)`.
#[rtse::function(module = "node:util", value = "toUSVString")]
fn to_usv_string_fn(str: &str) -> String {
    to_usv_string(str)
}

/// `util.stripVTControlCharacters(str)`.
#[rtse::function(module = "node:util", value = "stripVTControlCharacters")]
fn strip_vt_control_characters(str: &str) -> String {
    strip_vt(str)
}

/// `util.getSystemErrorName(err)`.
#[rtse::function(module = "node:util", value = "getSystemErrorName")]
fn get_system_error_name(err: i64) -> String {
    error_name(err)
}

/// `util.styleText(format, text)`.
#[rtse::function(module = "node:util", value = "styleText")]
fn style_text_fn(format: &str, text: &str) -> String {
    style_text(format, text)
}
