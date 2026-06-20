//! The Registry POPULATION tables — the data the engine registers at startup.
//!
//! Split out of `registry.rs` (which keeps the RESOLUTION API: `class_member`,
//! `namespace_member`, …) so the two responsibilities stop sharing one file. This
//! file is the conflict-prone one — every feature adds a namespace, a class spec,
//! or a `.ts` prelude — so it is structured as APPEND-MOSTLY DATA TABLES instead
//! of a long function body: adding a registrar is ONE row, not an edit in the
//! middle of imperative code, which makes parallel work collide far less.
//!
//! Two tables, both iterated in order by [`populate`]:
//!  - [`REGISTRATIONS`] — every `fn(&mut Engine)` that pushes namespace/class metadata
//!    (`ns::io::register`, `ns::globals::date::register_class_spec`, …). All share
//!    the `fn(&mut Engine)` shape, so a namespace and a class-spec are the same row
//!    kind. ORDER among these does not matter (each just pushes into the Registry).
//!  - [`PRELUDE_TS`] — the embedded `.ts` preludes included via `Engine::include`.
//!    ORDER MATTERS here: the merged prelude is one program, so a base must precede
//!    a dependent (`ERROR_TS` before the `extends Error` subclasses; primordials
//!    before `rts:test`'s `BUNDLE_TS`). Keep new entries in dependency order.
//!
//! The per-entry doc that used to be a big inline comment lives in the `why` field
//! (REGISTRATIONS) / the comment column (PRELUDE_TS) — one line, greppable.

use rts_engine::Engine;
use rts_runtime::namespaces as ns;

/// One registrar row: a short label (for greppability / docs) + the `fn(&mut Engine)`
/// that pushes its metadata into the Registry.
///
/// `label`/`why` are INTENTIONAL in-table documentation (the one-line replacement
/// for the old inline comments) — read by humans + grep, not by code; hence the
/// allow. Keeping them as data (not comments) means a row is self-describing.
#[allow(dead_code)]
pub(super) struct Registration {
    /// Short identity (the namespace / class name) — documentation + grep anchor.
    pub label: &'static str,
    /// The registrar fn (a namespace `register` or a class-spec `register_*`).
    pub run: fn(&mut Engine),
    /// One line: WHAT this registers and WHY (was the inline comment).
    pub why: &'static str,
}

/// Every `fn(&mut Engine)` registrar, in any order (each just pushes metadata).
/// ADD A NAMESPACE/CLASS HERE — one row, append-friendly. Heavier/feature-gated
/// namespaces (http_server/tls/ui/runtime) are deliberately absent until a test
/// needs them. `console` is NOT here — it is a `.ts` prelude (see [`PRELUDE_TS`]).
pub(super) static REGISTRATIONS: &[Registration] = &[
    // Namespaces backing class statics/ctors (Date.now/UTC/parse → `date`; Map/Set
    // → `collections`).
    Registration { label: "date", run: ns::date::register, why: "backs Date statics/ctors" },
    Registration { label: "collections", run: ns::collections::register, why: "backs Map/Set" },
    Registration { label: "regex", run: ns::regex::register, why: "backs RegExp" },
    // BUILTIN-IMPORT namespaces (`import { print } from "rts:io"`) — the public std
    // surface resolved via `namespace_member`; their `__RTS_FN_NS_*` real fn_ptrs are
    // harvested + JIT-installed by `all_jit_symbols`/`runtime_link`.
    Registration { label: "io", run: ns::io::register, why: "rts:io print/eprint/stdio" },
    Registration { label: "math", run: ns::math::register, why: "rts:math + Math statics" },
    Registration { label: "gc", run: ns::gc::register, why: "string pool / handle surface" },
    // `rts:test` framework backing namespaces, used ambiently by the embedded bundle
    // (prelude-gated in `method::try_method_dispatch`).
    Registration { label: "test", run: ns::test::register, why: "test_core.* runner primitives" },
    Registration { label: "globals::string", run: ns::globals::string::register, why: "string.* matchers" },
    Registration { label: "fmt", run: ns::fmt::register, why: "fmt.parse_f64 numeric matchers" },
    // The broad std surface `tests/*.test.ts` import via `rts:<ns>`.
    Registration { label: "fs", run: ns::fs::register, why: "rts:fs" },
    Registration { label: "time", run: ns::time::register, why: "rts:time" },
    Registration { label: "env", run: ns::env::register, why: "rts:env" },
    Registration { label: "path", run: ns::path::register, why: "rts:path" },
    Registration { label: "num", run: ns::num::register, why: "rts:num" },
    Registration { label: "mem", run: ns::mem::register, why: "rts:mem" },
    Registration { label: "hash", run: ns::hash::register, why: "rts:hash" },
    Registration { label: "hint", run: ns::hint::register, why: "rts:hint" },
    Registration { label: "ptr", run: ns::ptr::register, why: "rts:ptr" },
    Registration { label: "buffer", run: ns::buffer::register, why: "rts:buffer" },
    Registration { label: "alloc", run: ns::alloc::register, why: "rts:alloc" },
    Registration { label: "bigfloat", run: ns::bigfloat::register, why: "rts:bigfloat" },
    Registration { label: "atomic", run: ns::atomic::register, why: "rts:atomic" },
    Registration { label: "sync", run: ns::sync::register, why: "rts:sync" },
    Registration { label: "trace", run: ns::trace::register, why: "rts:trace" },
    Registration { label: "process", run: ns::process::register, why: "rts:process" },
    Registration { label: "os", run: ns::os::register, why: "rts:os" },
    Registration { label: "crypto", run: ns::crypto::register, why: "rts:crypto" },
    Registration { label: "net", run: ns::net::register, why: "rts:net" },
    Registration { label: "json", run: ns::json::register, why: "rts:json namespace" },
    Registration { label: "promise", run: ns::promise::register, why: "rts:promise" },
    Registration { label: "parallel", run: ns::parallel::register, why: "rts:parallel" },
    Registration { label: "thread", run: ns::thread::register, why: "rts:thread" },
    Registration { label: "ffi", run: ns::ffi::register, why: "rts:ffi" },
    Registration { label: "events", run: ns::events::register, why: "rts:events EventEmitter" },
    // `audio` — cross-platform audio backend; metadata-only wiring, engine never
    // NAMES the surface (resolves via Registry). `asio_audio` stays feature-gated.
    Registration { label: "audio", run: ns::audio::register, why: "rts:audio device I/O" },
    // PRIVATE `engine` namespace (arch/time/trace + the str_*/num_*/display/print_line
    // bridges) — prelude-only via the `engineobj` privacy gate.
    Registration { label: "engine", run: ns::engine::register, why: "private prelude bridges" },
    // RUNTIME/Registry global CLASS specs (ctor/methods/instanceof as metadata).
    Registration { label: "Date class", run: ns::globals::date::register_class_spec, why: "Date class spec" },
    Registration { label: "RegExp class", run: ns::globals::regexp::register_regexp_class_spec, why: "RegExp class spec" },
    // Error is NOT here — it is a `.ts` prelude (ERROR_TS). Boolean/Number/String
    // class specs let the `new X(..)` WRAPPER ctor resolve through the Registry.
    Registration { label: "Boolean class", run: ns::globals::boolean::register_boolean_class_spec, why: "Boolean wrapper ctor" },
    Registration { label: "Number class", run: ns::globals::number::register_number_class_spec, why: "Number wrapper ctor" },
    Registration { label: "String class", run: ns::globals::string::register_string_class_spec, why: "String wrapper ctor" },
];

/// One `.ts` prelude include, in DEPENDENCY ORDER (base before dependent).
/// `label`/`why` are in-table documentation (see [`Registration`]).
#[allow(dead_code)]
pub(super) struct PreludeTs {
    pub label: &'static str,
    pub source: &'static str,
    pub why: &'static str,
}

/// The embedded `.ts` preludes, included IN THIS ORDER (the merged prelude is one
/// program; a base class must precede its dependents). ADD A PRELUDE HERE — mind
/// the order: primordials first, framework (`rts:test`) last.
pub(super) static PRELUDE_TS: &[PreludeTs] = &[
    // PRIMORDIAL Error family — BEFORE anything that `extends Error`.
    PreludeTs { label: "Error", source: rts_runtime::ERROR_TS, why: "Error + subclasses (shape-based)" },
    // PRIMORDIAL Object instance methods + factory.
    PreludeTs { label: "Object", source: rts_runtime::OBJECT_TS, why: "hasOwnProperty/toString/valueOf" },
    // PRIMITIVE method libs (receiver boxed as `this`).
    PreludeTs { label: "Boolean", source: rts_runtime::BOOLEAN_TS, why: "bool.toString/valueOf" },
    PreludeTs { label: "Number", source: rts_runtime::NUMBER_TS, why: "num.toFixed/toString(radix)/…" },
    PreludeTs { label: "String", source: rts_runtime::STRING_TS, why: "str.toUpperCase/slice/indexOf/…" },
    // Global console object (front names nothing; bridges via engine.*).
    PreludeTs { label: "console", source: rts_runtime::CONSOLE_TS, why: "console.log/warn/… → engine bridges" },
    // Stdlib classes.
    PreludeTs { label: "Map/Set", source: rts_runtime::stdlib::MAP_SET_TS, why: "class Map/Set shadow native" },
    PreludeTs { label: "JSON", source: rts_runtime::stdlib::JSON_TS, why: "JSON.stringify/parse" },
    // The rts:test FRAMEWORK — LAST, so its Matcher/describe/test see every primordial.
    PreludeTs { label: "rts:test", source: rts_runtime::namespaces::test::BUNDLE_TS, why: "describe/test/expect/Matcher" },
];

/// Run every registrar then include every prelude, in table order. The whole
/// content of the former `build_registry` body — now data-driven.
pub(super) fn populate(e: &mut Engine) {
    for r in REGISTRATIONS {
        (r.run)(e);
    }
    for p in PRELUDE_TS {
        e.include(p.source);
    }
}
