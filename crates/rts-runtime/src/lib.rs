pub mod abi {
    pub use rts_engine::abi::*;
}

pub use rts_std::runtime;
/// Default HTTP User-Agent (`Rts v<version>` / `Rts development`), re-exported
/// from `rts-std`'s fetch backend so the CLI URL runner and the global `fetch()`
/// identify with the SAME agent string.
pub use rts_std::globals::fetch::default_user_agent as fetch_user_agent;
/// N-API (`.node` native addons). Re-exports the `rts-napi` crate: the `napi_*`
/// symbols + the loader `__RTS_FN_NS_NAPI_LOAD_ADDON`. The `require('x.node')`
/// lowering that calls the loader is the remaining integration on the new engine.
/// See docs/specs/napi-implementation.md.
pub use rts_napi as napi;
/// Embedded TS stdlib sources (engine includes), re-exported from `rts-shared`
/// so the new engine reaches them through the facade (`rts_runtime::stdlib`).
pub use rts_shared::stdlib;
/// Embedded TS source of the PRIMORDIAL `Error` family, re-exported from
/// `rts-primitives` (Error is a primordial, so its `.ts` lives there) so the new
/// engine reaches it through the facade (`rts_runtime::ERROR_TS`). Included by the
/// engine ahead of the Map/Set stdlib prelude.
pub use rts_primitives::ERROR_TS;
/// Embedded TS source of the PRIMORDIAL `Object` instance-method library +
/// factory, re-exported from `rts-primitives` so the new engine reaches it through
/// the facade (`rts_runtime::OBJECT_TS`). Included as a declarations-only prelude;
/// an object-receiver method call routes into its ambient `class Object`.
pub use rts_primitives::OBJECT_TS;
/// Embedded TS source of the PRIMORDIAL `Boolean.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::BOOLEAN_TS`). Included by the engine as a declarations-only
/// prelude; a primitive-bool method call routes into its ambient `class Boolean`.
pub use rts_primitives::BOOLEAN_TS;
/// Embedded TS source of the PRIMORDIAL `Number.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::NUMBER_TS`). Included as a declarations-only prelude; a
/// primitive-number method call routes into its ambient `class Number`. The
/// irreducible numeric formatting stays in Rust and is bridged via the private
/// `engine.num_*` helpers the `.ts` bodies call.
pub use rts_primitives::NUMBER_TS;
/// Embedded TS source of the PRIMORDIAL `String.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::STRING_TS`). Included as a declarations-only prelude; a
/// primitive-string method call routes into its ambient `class String`. The
/// irreducible Unicode string logic stays in Rust and is bridged via the private
/// `engine.str_*` helpers the `.ts` bodies call.
pub use rts_primitives::STRING_TS;
/// Embedded TS source of the global `console` object, re-exported from
/// `rts-shared` (it is a NON-primordial backend utility, not a primordial) so the
/// new engine reaches it through the facade (`rts_runtime::CONSOLE_TS`). Included
/// as a prelude; `console.log(...)` is an ordinary member call on this ambient
/// object — the front-end names nothing.
pub use rts_shared::stdlib::CONSOLE_TS;
pub mod namespaces;

/// One-time runtime bootstrap, called once at program startup before the lowered
/// top-level (`__rtsn_main`). The AOT `main` shim calls this symbol; the JIT path
/// calls [`runtime_init`] directly host-side.
///
/// It lives in the FACADE (not `rts-std`) because it wires TWO subsystems from
/// different crates: the GC bootstrap (`rts-std`'s `collector::runtime_init` —
/// install the GC tick hook + register the main thread) AND the UI render/input
/// backend (`rts-egui`'s `register_backend` — the egui backend lives in a
/// `thread_local!` set by the namespace `register()`, which only runs at compile
/// time, so an AOT binary must install it here, on the main thread, or `render.*`
/// draws nothing). Keeping it in the facade lets the ENGINE name ONE generic
/// `RT`-scope symbol and never name `egui`/`render` (PRIMORDIAL-vs-Registry).
///
/// Idempotent: the GC hook is a `OnceLock::set`, `register_current` de-dups by
/// thread id, and `register_backend` just overwrites the backend box.
pub fn runtime_init() {
    // Opt-in AOT startup profiling: `RTS_AOT_TIMING=1` prints the wall time of
    // each init phase to stderr. A no-op AOT binary's floor is dominated by
    // whatever runs here plus process/DLL load; this attributes it.
    let timing = std::env::var("RTS_AOT_TIMING").map(|v| v == "1").unwrap_or(false);
    let t0 = std::time::Instant::now();
    rts_std::collector::collector::runtime_init();
    if timing {
        eprintln!("[aot-init] gc+thread   {:.2} ms", t0.elapsed().as_secs_f64() * 1e3);
    }
    let t1 = std::time::Instant::now();
    rts_egui::render_backend::register_backend();
    if timing {
        eprintln!("[aot-init] egui backend {:.2} ms", t1.elapsed().as_secs_f64() * 1e3);
    }
}

/// `extern "C"` entry the AOT `main` shim calls before `__rtsn_main`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INIT() {
    runtime_init();
}

/// Seed the AOT binary's global shape + error-class registries from the blob the
/// codegen embedded (`rts_engine::heap::shapes::export_seed_blob`). Called by the
/// AOT `main` shim BEFORE `__rtsn_main`. Without it the registry is EMPTY in the
/// separate AOT process and every DYNAMIC shape read (`obj_get` on a Tagged/`any`
/// receiver, `console.log(obj)`, catch-bound `e.name`, `new Number(x).valueOf()`)
/// misses on its baked shape id and returns `undefined`. JIT does not need this —
/// it shares the compile-time registry in-process. Safe: `ptr`/`len` name a
/// codegen-emitted, linker-placed data object (never user input).
///
/// # Safety
/// `ptr` must point at `len` valid bytes produced by `export_seed_blob`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __RTS_FN_RT_SEED_SHAPES(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    rts_engine::heap::shapes::seed_from_blob(bytes);
}

