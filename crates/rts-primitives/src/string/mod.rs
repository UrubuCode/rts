//! String — namespace ABI + GlobalClassSpec para o tipo primitivo JS String.
//!
//! Migrado do `#[rts_namespace]` + `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Todos os
//! membros (namespace + classe) são `external`: os externs
//! `__RTS_FN_NS_STRING_*` (namespace) e `__RTS_FN_GL_STRING_*` (classe JS)
//! ficam em search/transform/replace/split.rs + rt.rs intactos; aqui só
//! montamos o `register` (namespace) + o `register_string_class_spec` (classe).
//!
//! Metodos de namespace (rts:string) em search/transform/replace/split.
//! Metodos de instancia JS (str.slice, str.split, etc.) em rt.rs.

pub mod replace;
pub mod rt;
pub mod search;
pub mod split;
pub mod strops;
pub mod transform;
pub mod value_class;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Endereço real do extern `__RTS_FN_NS_STRING_*` (namespace `string`). Esses
/// membros eram "external" (fn_ptr null), supridos à mão pelo `runtime_link` do
/// motor novo; agora o `register` carrega o endereço real e o HARVEST do Registry
/// (`all_jit_symbols`) instala o símbolo JIT — sem lista-mão no motor. Os externs
/// vivem nas submódulos search/transform/replace/split deste mesmo crate. Símbolos
/// de CLASSE (`__RTS_FN_GL_STRING_*`) seguem null aqui (instalados noutro caminho).
fn fp_for(symbol: &str) -> *const u8 {
    match symbol {
        "__RTS_FN_NS_STRING_CONTAINS" => search::__RTS_FN_NS_STRING_CONTAINS as *const u8,
        "__RTS_FN_NS_STRING_STARTS_WITH" => search::__RTS_FN_NS_STRING_STARTS_WITH as *const u8,
        "__RTS_FN_NS_STRING_ENDS_WITH" => search::__RTS_FN_NS_STRING_ENDS_WITH as *const u8,
        "__RTS_FN_NS_STRING_FIND" => search::__RTS_FN_NS_STRING_FIND as *const u8,
        "__RTS_FN_NS_STRING_TO_UPPER" => transform::__RTS_FN_NS_STRING_TO_UPPER as *const u8,
        "__RTS_FN_NS_STRING_TO_LOWER" => transform::__RTS_FN_NS_STRING_TO_LOWER as *const u8,
        "__RTS_FN_NS_STRING_TRIM" => transform::__RTS_FN_NS_STRING_TRIM as *const u8,
        "__RTS_FN_NS_STRING_TRIM_START" => transform::__RTS_FN_NS_STRING_TRIM_START as *const u8,
        "__RTS_FN_NS_STRING_TRIM_END" => transform::__RTS_FN_NS_STRING_TRIM_END as *const u8,
        "__RTS_FN_NS_STRING_REPEAT" => transform::__RTS_FN_NS_STRING_REPEAT as *const u8,
        "__RTS_FN_NS_STRING_REPLACE" => replace::__RTS_FN_NS_STRING_REPLACE as *const u8,
        "__RTS_FN_NS_STRING_REPLACEN" => replace::__RTS_FN_NS_STRING_REPLACEN as *const u8,
        "__RTS_FN_NS_STRING_BYTE_LEN" => split::__RTS_FN_NS_STRING_BYTE_LEN as *const u8,
        "__RTS_FN_NS_STRING_CHAR_AT" => split::__RTS_FN_NS_STRING_CHAR_AT as *const u8,
        "__RTS_FN_NS_STRING_CHAR_CODE_AT" => split::__RTS_FN_NS_STRING_CHAR_CODE_AT as *const u8,
        "__RTS_FN_NS_STRING_CHAR_COUNT" => split::__RTS_FN_NS_STRING_CHAR_COUNT as *const u8,
        // NOTE: the `__RTS_FN_GL_STRING_*` CLASS methods are NOT mapped here — they
        // are NOT registered as harvestable members; the engine's lowering emits a
        // few (slice/substring/substr/codePointAt/localeCompare) DIRECTLY, so they
        // stay in the engine's `adapter_symbols` list, and the rest route via the
        // `.ts` String class (Rust→Rust, no JIT symbol). fp_for stays NS-only.
        _ => core::ptr::null(),
    }
}

/// Membro de namespace/classe (helper hand-written, espelha a macro). O `fn_ptr`
/// é resolvido por [`fp_for`]: real para os externs de NAMESPACE (harvestados),
/// null para os de classe (`__RTS_FN_GL_STRING_*`, instalados noutro caminho).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp_for(symbol)),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Registra a namespace `string` no motor (hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("string")
        .doc("Rich string operations beyond the basic gc pool.")
        .member(m(
            "contains",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_NS_STRING_CONTAINS",
            "contains(haystack: string, needle: string): boolean",
            "True when `haystack` contains `needle`.",
            true,
        ))
        .member(m(
            "starts_with",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_NS_STRING_STARTS_WITH",
            "starts_with(s: string, prefix: string): boolean",
            "True when `s` starts with `prefix`.",
            true,
        ))
        .member(m(
            "ends_with",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_NS_STRING_ENDS_WITH",
            "ends_with(s: string, suffix: string): boolean",
            "True when `s` ends with `suffix`.",
            true,
        ))
        .member(m(
            "find",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_NS_STRING_FIND",
            "find(s: string, needle: string): number",
            "Byte index of first occurrence of `needle`, or -1 when absent.",
            true,
        ))
        .member(m(
            "to_upper",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_STRING_TO_UPPER",
            "to_upper(s: string): string",
            "Uppercase copy (Unicode-aware).",
            true,
        ))
        .member(m(
            "to_lower",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_STRING_TO_LOWER",
            "to_lower(s: string): string",
            "Lowercase copy (Unicode-aware).",
            true,
        ))
        .member(m(
            "trim",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_STRING_TRIM",
            "trim(s: string): string",
            "Removes ASCII + Unicode whitespace from both ends.",
            true,
        ))
        .member(m(
            "trim_start",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_STRING_TRIM_START",
            "trim_start(s: string): string",
            "Removes whitespace from the start.",
            true,
        ))
        .member(m(
            "trim_end",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_STRING_TRIM_END",
            "trim_end(s: string): string",
            "Removes whitespace from the end.",
            true,
        ))
        .member(m(
            "repeat",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::I64], AbiType::Handle),
            "__RTS_FN_NS_STRING_REPEAT",
            "repeat(s: string, n: number): string",
            "Concatenates `s` with itself `n` times.",
            true,
        ))
        .member(m(
            "replace",
            MemberKind::Function,
            Sig::new(
                vec![AbiType::StrPtr, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Handle,
            ),
            "__RTS_FN_NS_STRING_REPLACE",
            "replace(s: string, from: string, to: string): string",
            "Replaces every occurrence of `from` with `to`.",
            true,
        ))
        .member(m(
            "replacen",
            MemberKind::Function,
            Sig::new(
                vec![
                    AbiType::StrPtr,
                    AbiType::StrPtr,
                    AbiType::StrPtr,
                    AbiType::I64,
                ],
                AbiType::Handle,
            ),
            "__RTS_FN_NS_STRING_REPLACEN",
            "replacen(s: string, from: string, to: string, n: number): string",
            "Replaces the first `n` occurrences of `from` with `to`.",
            true,
        ))
        .member(m(
            "char_count",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_NS_STRING_CHAR_COUNT",
            "char_count(s: string): number",
            "Unicode codepoint count (chars).",
            true,
        ))
        .member(m(
            "byte_len",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_NS_STRING_BYTE_LEN",
            "byte_len(s: string): number",
            "Length in UTF-8 bytes.",
            true,
        ))
        .member(m(
            "char_at",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::I64], AbiType::Handle),
            "__RTS_FN_NS_STRING_CHAR_AT",
            "char_at(s: string, idx: number): string",
            "Character at Unicode index `idx` as a single-char string handle, or 0 out of range.",
            true,
        ))
        .member(m(
            "char_code_at",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::I64], AbiType::I64),
            "__RTS_FN_NS_STRING_CHAR_CODE_AT",
            "char_code_at(s: string, idx: number): number",
            "Unicode code point at `idx`, or -1 out of range.",
            true,
        ))
        .done();
}

/// Registra a classe global `String` no motor.
///
/// MIGRADO (2026-07-25): a superfície JS da classe agora é AUTORADA em Rust puro,
/// FULLY SELF-CONTAINED, via `#[rtse::class("String", value)]` em [`value_class`]
/// (lógica pura em [`strops`]). A `rts-macro` gera TODO símbolo ABI; os corpos
/// computam tudo em Rust — SEM delegação aos externs legados `__RTS_FN_GL_STRING_*`
/// (obsoletos, drenados à parte) e SEM `.member(...)` à mão. Esta fn é um wrapper
/// FINO sobre o `register` gerado pela macro (mantém o NOME: o codegen reroteia
/// pra cá).
///
/// COBRE todo `string.ts` MAIS `slice`/`substring`/`substr`/`codePointAt`/
/// `localeCompare` (para o codegen drenar seu dispatch hardcoded de String).
/// EXCLUÍDOS (arg[0] é RegExp, ficam no caminho regex/`dispatch.rs`):
/// `match`/`matchAll`/`search`/`split`.
pub fn register_string_class_spec(e: &mut Engine) {
    value_class::register(e);
}
