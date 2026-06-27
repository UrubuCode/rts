//! The Registry POPULATION tables — the data the engine registers at startup.
//!
//! Split out of `registry.rs` (which keeps the RESOLUTION API: `class_member`,
//! `namespace_member`, …) so the two responsibilities stop sharing one file. This
//! file is the conflict-prone one — every feature adds a namespace, a class spec,
//! or a `.ts` prelude — so it is structured as APPEND-MOSTLY DATA TABLES instead
//! of a long function body: adding a register is ONE row, not an edit in the
//! middle of imperative code, which makes parallel work collide far less.
//!
//! Two tables, both iterated in order by [`populate`]:
//!  - [`REGISTER`] — every `fn(&mut Engine)` that pushes namespace/class metadata
//!    (`ns::io::register`, `ns::globals::date::register_class_spec`, …). All share
//!    the `fn(&mut Engine)` shape, so a namespace and a class-spec are the same row
//!    kind. ORDER among these does not matter (each just pushes into the Registry).
//!  - [`PRELUDE_TS`] — the embedded `.ts` preludes included via `Engine::include`.
//!    ORDER MATTERS here: the merged prelude is one program, so a base must precede
//!    a dependent (`ERROR_TS` before the `extends Error` subclasses; primordials
//!    before `rts:test`'s `BUNDLE_TS`). Keep new entries in dependency order.
//!
//! The per-entry doc that used to be a big inline comment lives in the `why` field
//! (REGISTER) / the comment column (PRELUDE_TS) — one line, greppable.

use rts_engine::Engine;
use rts_runtime::namespaces as ns;

/// One register row: a short label (for greppability / docs) + the `fn(&mut Engine)`
/// that pushes its metadata into the Registry.
///
/// `label`/`why` are INTENTIONAL in-table documentation (the one-line replacement
/// for the old inline comments) — read by humans + grep, not by code; hence the
/// allow. Keeping them as data (not comments) means a row is self-describing.
#[allow(dead_code)]
pub(super) struct Register {
    /// Short identity (the namespace / class name) — documentation + grep anchor.
    pub label: &'static str,
    /// The register fn (a namespace `register` or a class-spec `register_*`).
    pub run: fn(&mut Engine),
    /// One line: WHAT this registers and WHY (was the inline comment).
    pub why: &'static str,
}

/// Every `fn(&mut Engine)` register, in any order (each just pushes metadata).
/// ADD A NAMESPACE/CLASS HERE — one row, append-friendly. Heavier/feature-gated
/// namespaces (http_server/tls/runtime) are deliberately absent until a test
/// needs them (`ui` is now registered — egui GUI, crate `rts-egui`). `console`
/// is NOT here — it is a `.ts` prelude (see [`PRELUDE_TS`]).
pub(super) static REGISTER: &[Register] = &[
    // Namespaces backing class statics/ctors (Date.now/UTC/parse → `date`; Map/Set
    // → `collections`).
    Register { label: "date", run: ns::date::register, why: "backs Date statics/ctors" },
    Register { label: "collections", run: ns::collections::register, why: "backs Map/Set" },
    Register { label: "regex", run: ns::regex::register, why: "backs RegExp" },
    // BUILTIN-IMPORT namespaces (`import { print } from "rts:io"`) — the public std
    // surface resolved via `namespace_member`; their `__RTS_FN_NS_*` real fn_ptrs are
    // harvested + JIT-installed by `all_jit_symbols`/`runtime_link`.
    Register { label: "io", run: ns::io::register, why: "rts:io print/eprint/stdio" },
    Register { label: "math", run: ns::math::register, why: "rts:math + Math statics" },
    Register { label: "gc", run: ns::gc::register, why: "string pool / handle surface" },
    // `rts:test` framework backing namespaces, used ambiently by the embedded bundle
    // (prelude-gated in `method::try_method_dispatch`).
    Register { label: "test", run: ns::test::register, why: "test_core.* runner primitives" },
    Register { label: "globals::string", run: ns::globals::string::register, why: "string.* matchers" },
    Register { label: "fmt", run: ns::fmt::register, why: "fmt.parse_f64 numeric matchers" },
    Register { label: "util", run: ns::fmt::register_node_util, why: "node:util compat (alias de fmt)" },
    // The broad std surface `tests/*.test.ts` import via `rts:<ns>`.
    Register { label: "fs", run: ns::fs::register, why: "rts:fs" },
    Register { label: "time", run: ns::time::register, why: "rts:time" },
    Register { label: "env", run: ns::env::register, why: "rts:env" },
    Register { label: "path", run: ns::path::register, why: "rts:path" },
    Register { label: "num", run: ns::num::register, why: "rts:num" },
    Register { label: "mem", run: ns::mem::register, why: "rts:mem" },
    Register { label: "hash", run: ns::hash::register, why: "rts:hash" },
    Register { label: "hint", run: ns::hint::register, why: "rts:hint" },
    Register { label: "ptr", run: ns::ptr::register, why: "rts:ptr" },
    Register { label: "buffer", run: ns::buffer::register, why: "rts:buffer" },
    Register { label: "alloc", run: ns::alloc::register, why: "rts:alloc" },
    Register { label: "bigfloat", run: ns::bigfloat::register, why: "rts:bigfloat" },
    Register { label: "atomic", run: ns::atomic::register, why: "rts:atomic" },
    Register { label: "sync", run: ns::sync::register, why: "rts:sync" },
    Register { label: "trace", run: ns::trace::register, why: "rts:trace" },
    Register { label: "process", run: ns::process::register, why: "rts:process" },
    Register { label: "os", run: ns::os::register, why: "rts:os" },
    Register { label: "crypto", run: ns::crypto::register, why: "rts:crypto" },
    Register { label: "net", run: ns::net::register, why: "rts:net" },
    Register { label: "json", run: ns::json::register, why: "rts:json namespace" },
    Register { label: "promise", run: ns::promise::register, why: "rts:promise" },
    Register { label: "parallel", run: ns::parallel::register, why: "rts:parallel" },
    Register { label: "thread", run: ns::thread::register, why: "rts:thread" },
    Register { label: "ffi", run: ns::ffi::register, why: "rts:ffi" },
    Register { label: "events", run: ns::events::register, why: "rts:events EventEmitter" },
    // `audio` — cross-platform audio backend; metadata-only wiring, engine never
    // NAMES the surface (resolves via Registry). `asio_audio` stays feature-gated.
    Register { label: "audio", run: ns::audio::register, why: "rts:audio device I/O" },
    // `dom` — DOM retido HEADLESS na crate `rts-dom`. Engine NUNCA nomeia `dom`;
    // resolve via Registry. parseHtml/querySelector/setText sem janela.
    Register { label: "dom", run: ns::dom::register, why: "rts:dom headless DOM primitives" },
    // `render` — interface de render ABSTRATA na crate `rts-render`. rect/text/
    // measureText despacham p/ o backend ativo (egui). O DOM/layout fala render.*.
    Register { label: "render", run: ns::render::register, why: "rts:render abstract backend" },
    // `input` — entrada ABSTRATA na crate IRMÃ `rts-input` (futuro: gamepad/touch).
    // O backend reporta o cru (polling); o DOM/layout faz hit-test + eventos. O TS
    // fala input.*, nunca egui.
    Register { label: "input", run: ns::input::register_input, why: "rts:input abstract polling" },
    // `egui` — GUI imediata na crate `rts-egui`. Engine NUNCA nomeia `egui`;
    // resolve via Registry. Loop dirigido pelo TS. Ver egui-ui-crate-design.md.
    Register { label: "egui", run: ns::egui::register, why: "rts:egui GUI primitives" },
    // PRIVATE `engine` namespace (arch/time/trace + the str_*/num_*/display/print_line
    // bridges) — prelude-only via the `engineobj` privacy gate.
    Register { label: "engine", run: ns::engine::register, why: "private prelude bridges" },
    // RUNTIME/Registry global CLASS specs (ctor/methods/instanceof as metadata).
    Register { label: "Date class", run: ns::globals::date::register_class_spec, why: "Date class spec" },
    // Promise is PRIMORDIAL; its class spec carries the static surface
    // (`resolve`/`reject`/`all`/…) + then/catch/finally as registry metadata so the
    // static-call path can resolve `Promise.resolve()` generically.
    Register { label: "Promise class", run: ns::globals::promise::register_promise_class_spec, why: "Promise statics + then/catch" },
    Register { label: "RegExp class", run: ns::globals::regexp::register_regexp_class_spec, why: "RegExp class spec" },
    // Error is NOT here — it is a `.ts` prelude (ERROR_TS). Boolean/Number/String
    // class specs let the `new X(..)` WRAPPER ctor resolve through the Registry.
    Register { label: "Boolean class", run: ns::globals::boolean::register_boolean_class_spec, why: "Boolean wrapper ctor" },
    Register { label: "Number class", run: ns::globals::number::register_number_class_spec, why: "Number wrapper ctor" },
    Register { label: "String class", run: ns::globals::string::register_string_class_spec, why: "String wrapper ctor" },
    // `URL` — backend/Registry class (WHATWG parser, no native syntax): `new URL(s)`
    // + getters (href/hostname/pathname/…) resolve generically via registryclass.
    Register { label: "URL class", run: ns::globals::url::register_url_class_spec, why: "URL ctor + getters" },
    // `URLSearchParams` — Registry class: `new URLSearchParams("a=1&b=2")` + get/has/
    // set/delete/toString. Os símbolos `__RTS_FN_GL_USP_*` já carregam fn_ptr real
    // (url::fp_for). `url.searchParams` retorna uma instância URLSearchParams →
    // resolvido pelo return-class tracking (ret_class "URLSearchParams").
    Register { label: "URLSearchParams class", run: ns::globals::url::register_urlsp_class_spec, why: "URLSearchParams ctor + get/has/set/delete" },
    // `TextEncoder`/`TextDecoder` — backend/Registry classes (UTF-8, no native
    // syntax): `new TextEncoder().encode(s)` → Uint8Array handle, `decode(h)` → str.
    Register { label: "TextEncoder class", run: ns::globals::text_encoding::register_text_encoder_class_spec, why: "TextEncoder ctor + encode" },
    Register { label: "TextDecoder class", run: ns::globals::text_encoding::register_text_decoder_class_spec, why: "TextDecoder ctor + decode" },
    // `EventEmitter` — backend/Registry class: `new EventEmitter([async])` + on/once/
    // off/emit. The listener arg is a function-VALUE the backend invokes via the
    // codegen `__rtsadp_fn_invoke` callback bridge (JIT-installed).
    Register { label: "EventEmitter class", run: ns::globals::events::register_class_spec, why: "EventEmitter ctor + on/emit" },
    // `Symbol` — NON-primordial Registry class (#216): `Symbol.for/keyFor` statics +
    // `Symbol.iterator`/… well-knowns + `description` getter resolve data-driven via
    // is_pure_registry_class (the engine NEVER names "Symbol" in control flow).
    Register { label: "Symbol class", run: ns::globals::symbol::register_symbol_class_spec, why: "Symbol.for/keyFor + well-knowns + description" },
    // `Proxy` — backend/Registry class (#218): `new Proxy(target, handler)` →
    // `__RTS_FN_GL_PROXY_NEW` (Entry::Proxy). The get/set TRAPS are resolved at
    // runtime in the dynamic property trampolines (`__rtsadp_obj_get`/`_set` detect
    // a Proxy receiver via `resolve_proxy` and invoke `handler.get`/`.set` through
    // the `__rtsadp_fn_invoke` callback bridge), not by a codegen arm.
    Register { label: "Proxy class", run: ns::globals::proxy::register_proxy_class_spec, why: "Proxy ctor → PROXY_NEW; traps in obj_get/set" },
    // `WeakRef` — Registry class (#217 A1.1): `new WeakRef(target)` → WEAKREF_NEW
    // (Entry::WeakRef, não traçado pelo coletor), `deref()` → WEAKREF_DEREF (weak
    // real: undefined após coleta). A ctor recebe o objeto-target (Handle) e o
    // deref RETORNA um objeto (Handle não-string → JsKind::Object via ret_is_string_handle).
    Register { label: "WeakRef class", run: ns::globals::weakref::register_weakref_class_spec, why: "WeakRef ctor + deref (weak)" },
    // `ArrayBuffer` + `DataView` — backend/Registry classes (raw bytes, no native
    // syntax): `new ArrayBuffer(n)` + `new DataView(buf)` + get/set<T>(offset[,v]).
    // Os membros do spec agora carregam fn_ptr real (dataview::fp_for) → o harvest
    // do Registry instala os símbolos no JIT (sem isso = "can't resolve symbol").
    Register { label: "ArrayBuffer class", run: ns::globals::dataview::register_array_buffer_class_spec, why: "ArrayBuffer ctor + byteLength/slice" },
    Register { label: "DataView class", run: ns::globals::dataview::register_data_view_class_spec, why: "DataView ctor + get/set accessors" },
];

/// One `.ts` prelude include, in DEPENDENCY ORDER (base before dependent).
/// `label`/`why` are in-table documentation (see [`Register`]).
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
    PreludeTs { label: "WeakMap/WeakSet", source: rts_runtime::stdlib::WEAKMAP_SET_TS, why: "class WeakMap/WeakSet (strong-ref, #217)" },
    PreludeTs { label: "JSON", source: rts_runtime::stdlib::JSON_TS, why: "JSON.stringify/parse" },
    // Reflect (get/set/has) — pure TS over dynamic property access + Object.keys
    // (#218). `Reflect.get`/`set` on a Proxy fire its traps via the dynamic
    // trampolines; after Object/JSON so its `Object.keys` resolves.
    PreludeTs { label: "Reflect", source: rts_runtime::stdlib::REFLECT_TS, why: "Reflect.get/set/has" },
    // structuredClone — global fn; after Map/Set/WeakMap/JSON so its instanceof +
    // Object.keys + Map/Set/Date clone resolve against the already-included classes.
    PreludeTs { label: "structuredClone", source: rts_runtime::stdlib::STRUCTURED_CLONE_TS, why: "deep clone w/ cycle detection" },
    // performance singleton — `.ts` over the private engine clock bridges (like console).
    PreludeTs { label: "performance", source: rts_runtime::stdlib::PERFORMANCE_TS, why: "performance.now()/timeOrigin" },
    // DOM facade — `Document`/`Element` classes (browser-named API) over the `rts:dom`
    // primitives. AFTER the namespaces register (the `dom.*` it calls must exist).
    PreludeTs { label: "DOM facade", source: ns::dom::DOM_TS, why: "document/Element over rts:dom" },
    // Canvas facade — ergonomic immediate-mode UI (Canvas: rect/text/button) over
    // the abstract render.*/input.* (NO DOM). AFTER render/input register.
    PreludeTs { label: "Canvas facade", source: ns::render::CANVAS_TS, why: "rts:canvas immediate UI over render.*" },
    // The rts:test FRAMEWORK — LAST, so its Matcher/describe/test see every primordial.
    PreludeTs { label: "rts:test", source: rts_runtime::namespaces::test::BUNDLE_TS, why: "describe/test/expect/Matcher" },
];

/// Run every register then include every prelude, in table order. The whole
/// content of the former `build_registry` body — now data-driven.
pub(super) fn populate(e: &mut Engine) {
    for r in REGISTER {
        (r.run)(e);
    }
    for p in PRELUDE_TS {
        e.include(p.source);
    }
}
