//! JIT install of the REAL `__RTS_FN_*` symbols for the RUNTIME/Registry classes
//! the engine dispatches through the Registry (Pilar 6).
//!
//! The Registry-driven lowering ([`crate::front::run::registry_call`]) emits a
//! direct `call __RTS_FN_GL_DATE_*` (etc.) — the ACTUAL runtime function, not a
//! codegen `__rtsadp_*` trampoline. The JIT must therefore resolve those symbols
//! to their real addresses. The Registry's own members carry a NULL `fn_ptr` for
//! these classes (they are declared `external`: the bodies live in the runtime
//! crates), exactly like the old engine — which supplies them through a hardcoded
//! `add_fn!` list. We do the equivalent honest thing: take each real function's
//! address through the `rts-runtime` facade. This is the LINK surface (a symbol →
//! address map the loader needs), NOT a per-class dispatch metadata table: the
//! method names, signatures, and arities all come from the Registry; only the raw
//! addresses are listed here (they have nowhere else to come from).

use rts_runtime::namespaces::date as rt_date;
use rts_runtime::namespaces::globals::date::instance as rt_gl_date;
use rts_runtime::namespaces::globals::url::instance as rt_gl_url;

use crate::adapter_symbols::JitSymbol;

/// Real `Date` class symbols (instance methods + ctors + the `date` namespace
/// statics backing `Date.now`/`UTC`/`parse`) the Registry resolves. One entry per
/// REAL extern the Registry's `Date` members reference by symbol.
pub fn date_symbols() -> Vec<JitSymbol> {
    macro_rules! s {
        ($name:literal, $f:path) => {
            JitSymbol {
                name: $name,
                ptr: $f as *const u8,
            }
        };
    }
    vec![
        // ── constructors ───────────────────────────────────────────────────
        s!(
            "__RTS_FN_GL_DATE_NEW_NOW",
            rt_gl_date::__RTS_FN_GL_DATE_NEW_NOW
        ),
        s!(
            "__RTS_FN_GL_DATE_NEW_FROM_MS",
            rt_gl_date::__RTS_FN_GL_DATE_NEW_FROM_MS
        ),
        s!(
            "__RTS_FN_GL_DATE_NEW_FROM_ISO",
            rt_gl_date::__RTS_FN_GL_DATE_NEW_FROM_ISO
        ),
        s!(
            "__RTS_FN_GL_DATE_NEW_FROM_FIELDS",
            rt_gl_date::__RTS_FN_GL_DATE_NEW_FROM_FIELDS
        ),
        // ── numeric getters ────────────────────────────────────────────────
        s!(
            "__RTS_FN_GL_DATE_GET_TIME",
            rt_gl_date::__RTS_FN_GL_DATE_GET_TIME
        ),
        s!(
            "__RTS_FN_GL_DATE_VALUE_OF",
            rt_gl_date::__RTS_FN_GL_DATE_VALUE_OF
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_FULL_YEAR",
            rt_gl_date::__RTS_FN_GL_DATE_GET_FULL_YEAR
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_MONTH",
            rt_gl_date::__RTS_FN_GL_DATE_GET_MONTH
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_DATE",
            rt_gl_date::__RTS_FN_GL_DATE_GET_DATE
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_DAY",
            rt_gl_date::__RTS_FN_GL_DATE_GET_DAY
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_HOURS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_HOURS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_MINUTES",
            rt_gl_date::__RTS_FN_GL_DATE_GET_MINUTES
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_SECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_SECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_MILLISECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_MILLISECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_FULL_YEAR",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_FULL_YEAR
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_MONTH",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_MONTH
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_DATE",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_DATE
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_DAY",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_DAY
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_HOURS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_HOURS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_MINUTES",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_MINUTES
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_SECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_SECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_UTC_MILLISECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_GET_UTC_MILLISECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET",
            rt_gl_date::__RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET
        ),
        // ── setters (registered methods; resolvable, fixtures avoid them) ───
        s!(
            "__RTS_FN_GL_DATE_SET_FULL_YEAR",
            rt_gl_date::__RTS_FN_GL_DATE_SET_FULL_YEAR
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_MONTH",
            rt_gl_date::__RTS_FN_GL_DATE_SET_MONTH
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_DATE",
            rt_gl_date::__RTS_FN_GL_DATE_SET_DATE
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_HOURS",
            rt_gl_date::__RTS_FN_GL_DATE_SET_HOURS
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_MINUTES",
            rt_gl_date::__RTS_FN_GL_DATE_SET_MINUTES
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_SECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_SET_SECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_MILLISECONDS",
            rt_gl_date::__RTS_FN_GL_DATE_SET_MILLISECONDS
        ),
        s!(
            "__RTS_FN_GL_DATE_SET_TIME",
            rt_gl_date::__RTS_FN_GL_DATE_SET_TIME
        ),
        // ── string conversions ─────────────────────────────────────────────
        s!(
            "__RTS_FN_GL_DATE_TO_ISO_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_ISO_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_JSON",
            rt_gl_date::__RTS_FN_GL_DATE_TO_JSON
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_UTC_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_UTC_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_DATE_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_DATE_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_TIME_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_TIME_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_LOCALE_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_LOCALE_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING
        ),
        s!(
            "__RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING",
            rt_gl_date::__RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING
        ),
        // ── the `date` namespace statics backing Date.now/UTC/parse ─────────
        s!("__RTS_FN_NS_DATE_NOW_MS", rt_date::__RTS_FN_NS_DATE_NOW_MS),
        s!(
            "__RTS_FN_NS_DATE_FROM_PARTS",
            rt_date::__RTS_FN_NS_DATE_FROM_PARTS
        ),
        s!(
            "__RTS_FN_NS_DATE_PARSE_F64",
            rt_date::__RTS_FN_NS_DATE_PARSE_F64
        ),
        // The `IS_DATE` tag predicate (Registry `instanceof_predicate`).
        s!(
            "__RTS_FN_NS_GC_IS_DATE",
            rts_runtime::namespaces::gc::string_pool::__RTS_FN_NS_GC_IS_DATE
        ),
    ]
}

/// Real `URL` class symbols (WHATWG parser): the `[StrPtr]`/`[StrPtr,StrPtr]` ctors
/// + the string getters (href/protocol/host/hostname/port/pathname/search/hash/
/// origin/username/password) the Registry `URL` members reference by symbol. The
/// class-spec Members carry null `fn_ptr` (the harvest skips them), so the real
/// extern addresses are installed here, like [`date_symbols`].
pub fn url_symbols() -> Vec<JitSymbol> {
    macro_rules! s {
        ($name:literal, $f:path) => {
            JitSymbol { name: $name, ptr: $f as *const u8 }
        };
    }
    vec![
        s!("__RTS_FN_GL_URL_NEW", rt_gl_url::__RTS_FN_GL_URL_NEW),
        s!("__RTS_FN_GL_URL_NEW_WITH_BASE", rt_gl_url::__RTS_FN_GL_URL_NEW_WITH_BASE),
        s!("__RTS_FN_GL_URL_HREF", rt_gl_url::__RTS_FN_GL_URL_HREF),
        s!("__RTS_FN_GL_URL_PROTOCOL", rt_gl_url::__RTS_FN_GL_URL_PROTOCOL),
        s!("__RTS_FN_GL_URL_HOST", rt_gl_url::__RTS_FN_GL_URL_HOST),
        s!("__RTS_FN_GL_URL_HOSTNAME", rt_gl_url::__RTS_FN_GL_URL_HOSTNAME),
        s!("__RTS_FN_GL_URL_PORT", rt_gl_url::__RTS_FN_GL_URL_PORT),
        s!("__RTS_FN_GL_URL_PATHNAME", rt_gl_url::__RTS_FN_GL_URL_PATHNAME),
        s!("__RTS_FN_GL_URL_SEARCH", rt_gl_url::__RTS_FN_GL_URL_SEARCH),
        s!("__RTS_FN_GL_URL_HASH", rt_gl_url::__RTS_FN_GL_URL_HASH),
        s!("__RTS_FN_GL_URL_ORIGIN", rt_gl_url::__RTS_FN_GL_URL_ORIGIN),
        s!("__RTS_FN_GL_URL_USERNAME", rt_gl_url::__RTS_FN_GL_URL_USERNAME),
        s!("__RTS_FN_GL_URL_PASSWORD", rt_gl_url::__RTS_FN_GL_URL_PASSWORD),
        s!("__RTS_FN_GL_URL_TO_STRING", rt_gl_url::__RTS_FN_GL_URL_TO_STRING),
        s!("__RTS_FN_GL_URL_FREE", rt_gl_url::__RTS_FN_GL_URL_FREE),
        s!("__RTS_FN_GL_URL_CAN_PARSE", rt_gl_url::__RTS_FN_GL_URL_CAN_PARSE),
        s!("__RTS_FN_GL_URL_CAN_PARSE_BASE", rt_gl_url::__RTS_FN_GL_URL_CAN_PARSE_BASE),
    ]
}
