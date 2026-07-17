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

/// Every `fn(&mut Engine)` register, in any order (each just pushes metadata).
/// ADD A NAMESPACE/CLASS HERE — one row (the bare register fn). Each fn NAMES
/// its own module/class inside (`e.ns("bigfloat")`, `e.ns("node:fs")`,
/// `e.class("Date")`), so this table carries NO label/why strings — the identity
/// lives in the data (the register fn), never duplicated here. Heavier/feature-
/// gated namespaces (http_server/runtime) are deliberately absent until a test
/// needs them. `console` is NOT here — it is a `.ts` prelude (see [`PRELUDE_TS`]).
pub(super) static REGISTER: &[fn(&mut Engine)] = &[
    // Namespaces backing class statics/ctors (Date.now/UTC/parse → `date`; Map/Set
    // → `collections`).
    ns::date::register,
    ns::collections::register,
    ns::regex::register,
    // BUILTIN-IMPORT namespaces (`import { print } from "rts:io"`) — the public std
    // surface resolved via `namespace_member`; their `__RTS_FN_NS_*` real fn_ptrs are
    // harvested + JIT-installed by `all_jit_symbols`/`runtime_link`.
    ns::io::register,
    ns::math::register,
    ns::gc::register,
    // `rts:test` framework backing namespaces, used ambiently by the embedded bundle
    // (prelude-gated in `method::try_method_dispatch`).
    ns::test::register,
    ns::globals::string::register,
    // Web Fetch API namespace — `fetch()` + `fetchText(url)` (sync HTTP GET via
    // ureq+TLS). Backs the mini-browser's real page download.
    ns::globals::fetch::register,
    // Image decode (PNG/JPEG/GIF/WebP) → RGBA8, for <img>/background-image.
    ns::globals::imgdec::register,
    ns::fmt::register,
    ns::fmt::register_node_util,
    // node:* modules — native rts-node crate (independent). `node:X` resolves here
    // via the same builtin-namespace path as `rts:X` (see docs/node-implementation/).
    ns::node_assert::register,
    ns::node_buffer::register,
    ns::node_child_process::register,
    ns::node_crypto::register,
    ns::node_dgram::register,
    ns::node_diagnostics_channel::register,
    ns::node_dns::register,
    ns::node_events::register,
    ns::node_fs::register,
    ns::node_http::register,
    ns::node_module::register,
    ns::node_net::register,
    ns::node_os::register,
    ns::node_path::register,
    ns::node_perf_hooks::register,
    ns::node_process::register,
    ns::node_punycode::register,
    ns::node_querystring::register,
    ns::node_string_decoder::register,
    ns::node_timers::register,
    ns::node_tls::register,
    ns::node_tty::register,
    ns::node_url::register,
    ns::node_util::register,
    ns::node_util::register_sys,
    ns::node_v8::register,
    ns::node_zlib::register,
    // The broad std surface `tests/*.test.ts` import via `rts:<ns>`.
    ns::fs::register,
    ns::time::register,
    ns::env::register,
    ns::path::register,
    ns::num::register,
    ns::mem::register,
    ns::hash::register,
    ns::hint::register,
    ns::ptr::register,
    ns::buffer::register,
    ns::alloc::register,
    ns::bigfloat::register,
    ns::atomic::register,
    ns::sync::register,
    ns::trace::register,
    ns::process::register,
    ns::os::register,
    ns::crypto::register,
    ns::net::register,
    ns::tls::register,
    ns::ws::register,
    ns::json::register,
    ns::promise::register,
    ns::thread::register,
    ns::ffi::register,
    ns::events::register,
    // `audio` — cross-platform audio backend; metadata-only wiring, engine never
    // NAMES the surface (resolves via Registry). `asio_audio` stays feature-gated.
    ns::audio::register,
    // `dom` — DOM retido HEADLESS na crate `rts-dom`. Engine NUNCA nomeia `dom`;
    // resolve via Registry. parseHtml/querySelector/setText sem janela.
    ns::dom::register,
    // `render` — interface de render ABSTRATA na crate `rts-render`. rect/text/
    // measureText despacham p/ o backend ativo (egui). O DOM/layout fala render.*.
    ns::render::register,
    // `input` — entrada ABSTRATA na crate IRMÃ `rts-input` (futuro: gamepad/touch).
    // O backend reporta o cru (polling); o DOM/layout faz hit-test + eventos. O TS
    // fala input.*, nunca egui.
    ns::input::register_input,
    // `egui` — GUI imediata na crate `rts-egui`. Engine NUNCA nomeia `egui`;
    // resolve via Registry. Loop dirigido pelo TS. Ver egui-ui-crate-design.md.
    ns::egui::register,
    // PRIVATE `engine` namespace (arch/time/trace + the str_*/num_*/display/print_line
    // bridges) — prelude-only via the `engineobj` privacy gate.
    ns::engine::register,
    // RUNTIME/Registry global CLASS specs (ctor/methods/instanceof as metadata).
    ns::globals::date::register_class_spec,
    // Promise is PRIMORDIAL; its class spec carries the static surface
    // (`resolve`/`reject`/`all`/…) + then/catch/finally as registry metadata so the
    // static-call path can resolve `Promise.resolve()` generically.
    ns::globals::promise::register_promise_class_spec,
    ns::globals::regexp::register_regexp_class_spec,
    // Error is NOT here — it is a `.ts` prelude (ERROR_TS). Boolean/Number/String
    // class specs let the `new X(..)` WRAPPER ctor resolve through the Registry.
    ns::globals::boolean::register_boolean_class_spec,
    ns::globals::number::register_number_class_spec,
    ns::globals::string::register_string_class_spec,
    // `URL` — backend/Registry class (WHATWG parser, no native syntax): `new URL(s)`
    // + getters (href/hostname/pathname/…) resolve generically via registryclass.
    ns::globals::url::register_url_class_spec,
    // `URLSearchParams` — Registry class: `new URLSearchParams("a=1&b=2")` + get/has/
    // set/delete/toString. Os símbolos `__RTS_FN_GL_USP_*` já carregam fn_ptr real
    // (url::fp_for). `url.searchParams` retorna uma instância URLSearchParams →
    // resolvido pelo return-class tracking (ret_class "URLSearchParams").
    ns::globals::url::register_urlsp_class_spec,
    // `TextEncoder`/`TextDecoder` — backend/Registry classes (UTF-8, no native
    // syntax): `new TextEncoder().encode(s)` → Uint8Array handle, `decode(h)` → str.
    ns::globals::text_encoding::register_text_encoder_class_spec,
    ns::globals::text_encoding::register_text_decoder_class_spec,
    // `EventEmitter` — backend/Registry class: `new EventEmitter([async])` + on/once/
    // off/emit. The listener arg is a function-VALUE the backend invokes via the
    // codegen `__rtsadp_fn_invoke` callback bridge (JIT-installed).
    ns::globals::events::register_class_spec,
    // `Symbol` — NON-primordial Registry class (#216): `Symbol.for/keyFor` statics +
    // `Symbol.iterator`/… well-knowns + `description` getter resolve data-driven via
    // is_pure_registry_class (the engine NEVER names "Symbol" in control flow).
    ns::globals::symbol::register_symbol_class_spec,
    // `Proxy` — backend/Registry class (#218): `new Proxy(target, handler)` →
    // `__RTS_FN_GL_PROXY_NEW` (Entry::Proxy). The get/set TRAPS are resolved at
    // runtime in the dynamic property trampolines (`__rtsadp_obj_get`/`_set` detect
    // a Proxy receiver via `resolve_proxy` and invoke `handler.get`/`.set` through
    // the `__rtsadp_fn_invoke` callback bridge), not by a codegen arm.
    ns::globals::proxy::register_proxy_class_spec,
    // `WeakRef` — Registry class (#217 A1.1): `new WeakRef(target)` → WEAKREF_NEW
    // (Entry::WeakRef, não traçado pelo coletor), `deref()` → WEAKREF_DEREF (weak
    // real: undefined após coleta). A ctor recebe o objeto-target (Handle) e o
    // deref RETORNA um objeto (Handle não-string → JsKind::Object via ret_is_string_handle).
    ns::globals::weakref::register_weakref_class_spec,
    // `ArrayBuffer` + `DataView` — backend/Registry classes (raw bytes, no native
    // syntax): `new ArrayBuffer(n)` + `new DataView(buf)` + get/set<T>(offset[,v]).
    // Os membros do spec agora carregam fn_ptr real (dataview::fp_for) → o harvest
    // do Registry instala os símbolos no JIT (sem isso = "can't resolve symbol").
    ns::globals::dataview::register_array_buffer_class_spec,
    ns::globals::dataview::register_data_view_class_spec,
    register_typed_array_class_specs,
];

/// Register the 8 TypedArray classes (Vec-backed level A — see
/// `rts_adapters::value::taops`). Each ctor takes ONE polymorphic arg (length /
/// source array / ArrayBuffer) and declares a `number[]` return, so the engine
/// tracks the instance as a plain ARRAY (length/index/`Array.from`/`join`/`at`/
/// `includes`/`slice` all ride the array surface; `set`/`subarray` are the two
/// TypedArray-only dispatch rows). Registered HERE (not rts-runtime) because
/// the ctor externs live in `rts-adapters` (they build PolyValue words), which
/// rts-runtime cannot depend on.
fn register_typed_array_class_specs(e: &mut Engine) {
    use rts_adapters::value::taops as ta;
    use rts_engine::{AbiType, FnPtr, Member, MemberFlags, MemberKind, Sig};
    let ctors: &[(&str, &str, *const u8)] = &[
        ("Uint8Array", "__RTS_FN_GL_TA_NEW_U8", ta::__RTS_FN_GL_TA_NEW_U8 as *const u8),
        ("Int8Array", "__RTS_FN_GL_TA_NEW_I8", ta::__RTS_FN_GL_TA_NEW_I8 as *const u8),
        ("Uint16Array", "__RTS_FN_GL_TA_NEW_U16", ta::__RTS_FN_GL_TA_NEW_U16 as *const u8),
        ("Int16Array", "__RTS_FN_GL_TA_NEW_I16", ta::__RTS_FN_GL_TA_NEW_I16 as *const u8),
        ("Uint32Array", "__RTS_FN_GL_TA_NEW_U32", ta::__RTS_FN_GL_TA_NEW_U32 as *const u8),
        ("Int32Array", "__RTS_FN_GL_TA_NEW_I32", ta::__RTS_FN_GL_TA_NEW_I32 as *const u8),
        ("Float32Array", "__RTS_FN_GL_TA_NEW_F32", ta::__RTS_FN_GL_TA_NEW_F32 as *const u8),
        ("Float64Array", "__RTS_FN_GL_TA_NEW_F64", ta::__RTS_FN_GL_TA_NEW_F64 as *const u8),
    ];
    for (class, symbol, ptr) in ctors {
        let m = |name: &str, argc: usize, sym: &str, fp: *const u8, ts: &str| Member {
            name: name.to_string(),
            kind: MemberKind::InstanceMethod,
            sig: Sig::new(vec![AbiType::PolyValue; argc + 1], AbiType::PolyValue),
            symbol: sym.to_string(),
            fn_ptr: FnPtr(fp),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: ts.to_string(),
            doc: "TypedArray instance surface (works on both the Vec-backed and the buffer-VIEW representations).".to_string(),
            pure: false,
            intrinsic: None,
        };
        e.class(class)
            .doc("TypedArray — Vec-backed for length/array ctors, a live buffer VIEW for an ArrayBuffer ctor.")
            .member(Member {
                name: "new".to_string(),
                kind: MemberKind::Constructor,
                sig: Sig::new(vec![AbiType::PolyValue], AbiType::Handle),
                symbol: symbol.to_string(),
                fn_ptr: FnPtr(*ptr),
                flags: MemberFlags::NONE,
                aliases: Vec::new(),
                variadic: false,
                ts_signature: format!("new {class}(src: number | number[] | ArrayBuffer): number[]"),
                doc: "Typed-array constructor (length → zeros; array → wrapped copy; ArrayBuffer → live view).".to_string(),
                pure: false,
                intrinsic: None,
            })
            .member(m(
                "set",
                2,
                "__rtsadp_arr_ta_set",
                ta::__rtsadp_arr_ta_set as *const u8,
                "set(src: number[], offset?: number): void",
            ))
            .member(m(
                "set",
                1,
                "__rtsadp_arr_ta_set1",
                ta::__rtsadp_arr_ta_set1 as *const u8,
                "set(src: number[]): void",
            ))
            .member(m(
                "subarray",
                2,
                "__rtsadp_arr_subarray",
                ta::__rtsadp_arr_subarray as *const u8,
                "subarray(begin?: number, end?: number): number[]",
            ))
            .member(m(
                "subarray",
                1,
                "__rtsadp_arr_subarray1",
                ta::__rtsadp_arr_subarray1 as *const u8,
                "subarray(begin: number): number[]",
            ))
            .member(Member {
                name: "length".to_string(),
                kind: MemberKind::InstanceGetter,
                sig: Sig::new(vec![AbiType::PolyValue], AbiType::PolyValue),
                symbol: "__rtsadp_dyn_length".to_string(),
                fn_ptr: FnPtr(rts_adapters::value::dyndispatch::__rtsadp_dyn_length as *const u8),
                flags: MemberFlags::NONE,
                aliases: Vec::new(),
                variadic: false,
                ts_signature: "length: number".to_string(),
                doc: "Element count (tag-dispatched: Vec length or buffer bytes / elem width).".to_string(),
                pure: true,
                intrinsic: None,
            })
            .done();
    }
    // `BigInt.asIntN/asUintN` statics — N-bit wrap over the i64 interim BigInt
    // model (#219; literals already lower as fits-i64 Ints).
    let mut bi = e.class("BigInt");
    for (name, sym, fp) in [
        ("asIntN", "__RTS_FN_GL_BIGINT_AS_INTN", ta::__rtsadp_bigint_as_intn as *const u8),
        ("asUintN", "__RTS_FN_GL_BIGINT_AS_UINTN", ta::__rtsadp_bigint_as_uintn as *const u8),
    ] {
        bi = bi.member(Member {
            name: name.to_string(),
            kind: MemberKind::StaticMethod,
            sig: Sig::new(vec![AbiType::PolyValue; 2], AbiType::PolyValue),
            symbol: sym.to_string(),
            fn_ptr: FnPtr(fp),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: format!("{name}(bits: number, v: bigint): bigint"),
            doc: "N-bit wrap (i64 interim BigInt model, #219).".to_string(),
            pure: true,
            intrinsic: None,
        });
    }
    bi.done();
    // `Atomics.*` statics (level A — single-threaded runtime: plain RMW on the
    // Vec-backed typed array; RMW ops return the PREVIOUS value, `store` the
    // stored value — exact JS observables).
    let statics: &[(&str, usize, *const u8)] = &[
        ("load", 2, ta::__rtsadp_atomics_load as *const u8),
        ("store", 3, ta::__rtsadp_atomics_store as *const u8),
        ("add", 3, ta::__rtsadp_atomics_add as *const u8),
        ("sub", 3, ta::__rtsadp_atomics_sub as *const u8),
        ("and", 3, ta::__rtsadp_atomics_and as *const u8),
        ("or", 3, ta::__rtsadp_atomics_or as *const u8),
        ("xor", 3, ta::__rtsadp_atomics_xor as *const u8),
        ("exchange", 3, ta::__rtsadp_atomics_exchange as *const u8),
        ("compareExchange", 4, ta::__rtsadp_atomics_cmpxchg as *const u8),
    ];
    let mut cls = e.class("Atomics");
    for (name, argc, ptr) in statics {
        cls = cls.member(Member {
            name: name.to_string(),
            kind: MemberKind::StaticMethod,
            sig: Sig::new(vec![AbiType::PolyValue; *argc], AbiType::PolyValue),
            symbol: format!("__RTS_FN_GL_TAATOMICS_{}", name.to_uppercase()),
            fn_ptr: FnPtr(*ptr),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: format!("{name}(...): number"),
            doc: "Atomics level A (single-threaded RMW).".to_string(),
            pure: false,
            intrinsic: None,
        });
    }
    cls.done();
}

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
    PreludeTs { label: "JSON5", source: rts_runtime::stdlib::JSON5_TS, why: "JSON5.parse (sanitizer → JSON.parse) + stringify" },
    PreludeTs { label: "Iterator", source: rts_runtime::stdlib::ITERATOR_TS, why: "Iterator.from + toArray cursor (#306 nível A)" },
    // Reflect (get/set/has) — pure TS over dynamic property access + Object.keys
    // (#218). `Reflect.get`/`set` on a Proxy fire its traps via the dynamic
    // trampolines; after Object/JSON so its `Object.keys` resolves.
    PreludeTs { label: "Reflect", source: rts_runtime::stdlib::REFLECT_TS, why: "Reflect.get/set/has" },
    // structuredClone — global fn; after Map/Set/WeakMap/JSON so its instanceof +
    // Object.keys + Map/Set/Date clone resolve against the already-included classes.
    PreludeTs { label: "structuredClone", source: rts_runtime::stdlib::STRUCTURED_CLONE_TS, why: "deep clone w/ cycle detection" },
    PreludeTs { label: "DOMException", source: rts_runtime::stdlib::DOMEXCEPTION_TS, why: "web exception class (name/message/legacy code)" },
    // performance singleton — `.ts` over the private engine clock bridges (like console).
    PreludeTs { label: "performance", source: rts_runtime::stdlib::PERFORMANCE_TS, why: "performance.now()/timeOrigin" },
    // Global timers — setTimeout/clearTimeout/setInterval/clearInterval/
    // queueMicrotask over the private engine timer bridges (ordered queues).
    PreludeTs { label: "timers", source: rts_runtime::stdlib::TIMERS_TS, why: "setTimeout/queueMicrotask globals" },
    // Web-platform value classes — pure `.ts` holders; after Object/JSON (their
    // ctors read `Object.keys` + `JSON.parse` from the merged prelude).
    PreludeTs { label: "web-api", source: rts_runtime::stdlib::WEBAPI_TS, why: "Headers/FormData/Blob/File/Request/Response" },
    // Web event model — after timers (AbortSignal.timeout → setTimeout;
    // MessagePort → queueMicrotask) and DOMException (abort/timeout reasons).
    PreludeTs { label: "events", source: rts_runtime::stdlib::EVENTS_TS, why: "Event/EventTarget/AbortSignal/AbortController/MessageChannel" },
    // Web Streams — after web-api (Blob.stream() builds a ReadableStream; the
    // UTF-8 helpers live in webapi.ts; the merged prelude is one program).
    PreludeTs { label: "streams", source: rts_runtime::stdlib::STREAMS_TS, why: "ReadableStream/WritableStream/TransformStream/TextEncoder/DecoderStream" },
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
    for run in REGISTER {
        run(e);
    }
    for p in PRELUDE_TS {
        e.include(p.source);
    }
    populate_runtime_ci(e);
}

/// Harvest every class `InstanceMethod` — and every `InstanceGetter`, which is
/// just a zero-arg method at the ABI — into the runtime `(class, member, arity)`
/// table so an UNTRACKED receiver (array element, param, any-typed local) can
/// dispatch object-backed Registry members at runtime via its own `__rts_class`
/// tag — the same `fn_ptr` the proven compile-time path resolves. Pure data, no
/// class named in the engine (the key is harvested from each `class.name`).
///
/// Getters matter because a COMPUTED property (`socket.remoteAddress`, which
/// reads the OS) has no stored field for the property path to find — unlike
/// `Stats`, whose values sit in the instance map. Without this, every computed
/// property on a callback's receiver read `undefined`.
fn populate_runtime_ci(e: &Engine) {
    use rts_engine::MemberKind;
    for class in e.registry().classes() {
        for m in &class.members {
            if matches!(m.kind, MemberKind::InstanceMethod | MemberKind::InstanceGetter)
                && !m.fn_ptr.0.is_null()
            {
                rts_engine::runtime_ci::register_ci(
                    &class.name,
                    &m.name,
                    m.sig.args.len(),
                    m.fn_ptr.0,
                    m.sig.args.clone(),
                    m.sig.returns,
                );
            }
        }
    }
}
