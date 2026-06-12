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
pub mod transform;

use rts_engine::abi::member::DefaultArg;
use rts_engine::{AbiType, Engine, FnPtr, Intrinsic, Member, MemberFlags, MemberKind, Sig};

/// Membro de namespace/classe (helper hand-written, espelha a macro). Como
/// todos os membros aqui são `external`, o `fn_ptr` é sempre null.
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
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
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

/// Registra a classe global `String` no motor (hand-written, sem macro).
pub fn register_string_class_spec(e: &mut Engine) {
    e.class("String")
        .doc("JS String primitive class — new String(), String.fromCharCode(), str.slice() etc.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_NEW_BOXED",
            "new(value: string): string",
            "new String(value) — wraps value como StringBox (typeof returns 'object').",
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_STRING_NEW_EMPTY",
            "new(): string",
            "new String() — empty string.",
            true,
        ))
        .member(m(
            "fromCharCode",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_STRING_FROM_CHAR_CODE",
            "fromCharCode(code: number): string",
            "String.fromCharCode(code) — char from UTF-16 code unit.",
            true,
        ))
        .member(m(
            "fromCodePoint",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_STRING_FROM_CODE_POINT",
            "fromCodePoint(codePoint: number): string",
            "String.fromCodePoint(codePoint) — char from full Unicode code point.",
            true,
        ))
        .member(m(
            "length",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_STRING_LENGTH_UTF16",
            "length: number",
            "str.length — number of UTF-16 code units (JS spec).",
            true,
        ))
        .member(m(
            "indexOf",
            MemberKind::InstanceMethod,
            // indexOf(needle, from? = 0): o símbolo _FROM com from=0 é idêntico
            // ao indexOf base (busca do início). Default no Registry cobre as
            // duas aridades — sem braço hardcoded por aridade no codegen.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::I64,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_INDEX_OF_FROM",
            "indexOf(needle: string, from?: number): number",
            "str.indexOf(needle, from) — first occurrence index, or -1.",
            true,
        ))
        .member(m(
            "lastIndexOf",
            MemberKind::InstanceMethod,
            // from? = i64::MAX (runtime clampa pra length = busca o fim todo).
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::I64,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_LAST_INDEX_OF_FROM",
            "lastIndexOf(needle: string): number",
            "str.lastIndexOf(needle) — last occurrence index, or -1.",
            true,
        ))
        .member(m(
            "includes",
            MemberKind::InstanceMethod,
            // includes(needle, pos? = 0): _AT com pos=0 == includes base.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::Bool,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_INCLUDES_AT",
            "includes(needle: string, pos?: number): boolean",
            "str.includes(needle, pos) — true when needle is found.",
            true,
        ))
        .member(m(
            "startsWith",
            MemberKind::InstanceMethod,
            // startsWith(prefix, pos? = 0): _AT com pos=0 == startsWith base.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::Bool,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_STARTS_WITH_AT",
            "startsWith(prefix: string, pos?: number): boolean",
            "str.startsWith(prefix, pos).",
            true,
        ))
        .member(m(
            "endsWith",
            MemberKind::InstanceMethod,
            // endPos? = i64::MAX (runtime clampa pra length = checa o fim).
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::Bool,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_ENDS_WITH_AT",
            "endsWith(suffix: string): boolean",
            "str.endsWith(suffix).",
            true,
        ))
        .member(m(
            "charAt",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_STRING_CHAR_AT",
            "charAt(idx: number): string",
            "str.charAt(idx).",
            true,
        ))
        .member(m(
            "charCodeAt",
            MemberKind::InstanceMethod,
            // idx? = 0; retorna F64 (NaN p/ fora de range, JS spec) via o extern
            // _F64. Antes o Registry usava o variant I64 (divergia do codegen).
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64],
                AbiType::F64,
                vec![DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_CHAR_CODE_AT_F64",
            "charCodeAt(idx?: number): number",
            "str.charCodeAt(idx) — UTF-16 code unit at index (NaN out of range).",
            true,
        ))
        .member(m(
            "codePointAt",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_STRING_CODE_POINT_AT",
            "codePointAt(idx: number): number",
            "str.codePointAt(idx) — full Unicode code point at index.",
            true,
        ))
        .member(m(
            "at",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_STRING_AT",
            "at(idx: number): string",
            "str.at(idx) — char at index (negative counts from end).",
            true,
        ))
        .member(m(
            "slice",
            MemberKind::InstanceMethod,
            // start? = 0, end? = i64::MAX (runtime clampa pra length = "até o
            // fim"). Defaults no Registry cobrem 0/1/2 args.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64, AbiType::I64],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Int(0), DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_SLICE",
            "slice(start?: number, end?: number): string",
            "str.slice(start, end) — negatives from end.",
            true,
        ))
        .member(m(
            "substring",
            MemberKind::InstanceMethod,
            // end? = i64::MAX (runtime clampa = "até o fim").
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64, AbiType::I64],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_SUBSTRING",
            "substring(start: number, end?: number): string",
            "str.substring(start, end) — like slice but clamps negatives to 0.",
            true,
        ))
        .member(m(
            "substr",
            MemberKind::InstanceMethod,
            // length? = i64::MAX (runtime clampa = "resto da string").
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64, AbiType::I64],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_SUBSTR",
            "substr(start: number, length?: number): string",
            "str.substr(start, length) — deprecated start+count form.",
            true,
        ))
        .member(m(
            "toUpperCase",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_UPPER_CASE",
            "toUpperCase(): string",
            "str.toUpperCase().",
            true,
        ))
        .member(m(
            "toLocaleUpperCase",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_UPPER_CASE",
            "toLocaleUpperCase(): string",
            "str.toLocaleUpperCase() — alias for toUpperCase.",
            true,
        ))
        .member(m(
            "toLowerCase",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_LOWER_CASE",
            "toLowerCase(): string",
            "str.toLowerCase().",
            true,
        ))
        .member(m(
            "toLocaleLowerCase",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_LOWER_CASE",
            "toLocaleLowerCase(): string",
            "str.toLocaleLowerCase() — alias for toLowerCase.",
            true,
        ))
        .member(m(
            "trim",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TRIM",
            "trim(): string",
            "str.trim().",
            true,
        ))
        .member(m(
            "trimStart",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TRIM_START",
            "trimStart(): string",
            "str.trimStart().",
            true,
        ))
        .member(m(
            "trimLeft",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TRIM_START",
            "trimLeft(): string",
            "str.trimLeft() — alias for trimStart.",
            true,
        ))
        .member(m(
            "trimEnd",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TRIM_END",
            "trimEnd(): string",
            "str.trimEnd().",
            true,
        ))
        .member(m(
            "trimRight",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TRIM_END",
            "trimRight(): string",
            "str.trimRight() — alias for trimEnd.",
            true,
        ))
        .member(m(
            "repeat",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_STRING_REPEAT",
            "repeat(n: number): string",
            "str.repeat(n).",
            true,
        ))
        .member(m(
            "replace",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::Handle, AbiType::Handle],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_STRING_REPLACE",
            "replace(from: string, to: string): string",
            "str.replace(from, to) — replace first occurrence.",
            true,
        ))
        .member(m(
            "replaceAll",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::Handle, AbiType::Handle],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_STRING_REPLACE_ALL",
            "replaceAll(from: string, to: string): string",
            "str.replaceAll(from, to) — replace all occurrences.",
            true,
        ))
        .member(m(
            "concat",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_CONCAT",
            "concat(other: string): string",
            "str.concat(other) — concatenate two strings.",
            true,
        ))
        .member(m(
            "padStart",
            MemberKind::InstanceMethod,
            // padString? = handle 0: o runtime PAD_START faz unwrap_or(" "),
            // então pad omitido (0) vira espaço. Default no Registry.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64, AbiType::Handle],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_PAD_START",
            "padStart(targetLength: number, padString?: string): string",
            "str.padStart(targetLength, padString).",
            true,
        ))
        .member(m(
            "padEnd",
            MemberKind::InstanceMethod,
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64, AbiType::Handle],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_PAD_END",
            "padEnd(targetLength: number, padString?: string): string",
            "str.padEnd(targetLength, padString).",
            true,
        ))
        .member(m(
            "split",
            MemberKind::InstanceMethod,
            // sep string OU regex (o runtime SPLIT_AUTO inspeciona o handle);
            // limit? = i64::MAX (sem limite). Dispatch por tipo-de-arg movido
            // pro runtime (rts-shared) — codegen chama UM símbolo genérico.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MAX)],
            ),
            "__RTS_FN_GL_STRING_SPLIT_AUTO",
            "split(sep: string | RegExp, limit?: number): string[]",
            "str.split(sep, limit) — sep string ou regex (dispatch no runtime).",
            true,
        ))
        // match/search/matchAll — pattern string OU regex; o dispatch por tipo
        // vive no runtime (_AUTO inspeciona Entry::Regex). Codegen genérico.
        .member(m(
            "match",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_MATCH_AUTO",
            "match(pattern: string | RegExp): string[] | null",
            "str.match(pattern) — string ou regex (dispatch no runtime).",
            true,
        ))
        .member(m(
            "search",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_STRING_SEARCH_AUTO",
            "search(pattern: string | RegExp): number",
            "str.search(pattern) — index do 1º match, ou -1 (dispatch no runtime).",
            true,
        ))
        .member(m(
            "matchAll",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_MATCH_ALL_AUTO",
            "matchAll(pattern: string | RegExp): string[][]",
            "str.matchAll(pattern) — todos os matches (dispatch no runtime).",
            true,
        ))
        .member(m(
            "localeCompare",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_STRING_LOCALE_COMPARE",
            "localeCompare(other: string): number",
            "str.localeCompare(other) — -1 / 0 / 1.",
            true,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_STRING",
            "toString(): string",
            "str.toString() — identity.",
            true,
        ))
        .member(Member {
            name: "valueOf".to_string(),
            kind: MemberKind::InstanceMethod,
            // primitivo (string literal/builtin path) → ReceiverIdentity devolve
            // a própria string; StringBox (var tipada String, Ident path) →
            // lower_global_instance_call chama BOX_VALUE_OF (unwrap), ignorando o
            // tag. Mesmo padrão de Number.valueOf.
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_STRING_BOX_VALUE_OF".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "valueOf(): string".to_string(),
            doc: "str.valueOf() — identity (primitive) / unwrap (StringBox)."
                .to_string(),
            pure: true,
            intrinsic: Some(Intrinsic::ReceiverIdentity),
        })
        .member(m(
            "isWellFormed",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_STRING_IS_WELL_FORMED",
            "isWellFormed(): boolean",
            "str.isWellFormed() — always true for RTS UTF-8 strings.",
            true,
        ))
        .member(m(
            "toWellFormed",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_STRING_TO_WELL_FORMED",
            "toWellFormed(): string",
            "str.toWellFormed() — substitui lone surrogates por U+FFFD.",
            true,
        ))
        .member(m(
            "normalize",
            MemberKind::InstanceMethod,
            // form? = handle 0 (runtime NORMALIZE faz default NFC). Antes o
            // Registry apontava pro stub TO_STRING (no-op) — divergia do codegen.
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_GL_STRING_NORMALIZE",
            "normalize(form?: string): string",
            "str.normalize() — stub: identity (full NFC/NFD is a follow-up).",
            true,
        ))
        .done();
}
