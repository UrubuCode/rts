//! Superfície N-API restante (15 stubs) — bloqueada pelo engine:
//! - threadsafe functions (8): precisam de fila MPSC + thread JS (event loop #207)
//! - bigint uint64/words (4): precisam de BigInt real (#219)
//! - uv_event_loop (1): precisa de shim libuv sobre tokio
//! - add/remove_async_cleanup_hook (1): teardown hooks do env
//! - module_register (1): registro legado, fora de escopo (não-N-API)
//! Stubs `napi_generic_failure` p/ o load_all() do napi-sys completar.
//! Ver docs/specs/napi-implementation.md / issue #1548.

#![allow(clippy::too_many_arguments)]
use crate::types::napi_status;
use napi_status::napi_generic_failure;

macro_rules! surface_stub {
    ($name:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            _a: usize, _b: usize, _c: usize, _d: usize, _e: usize, _f: usize,
        ) -> napi_status { napi_generic_failure }
    };
}

surface_stub!(napi_acquire_threadsafe_function);
surface_stub!(napi_add_async_cleanup_hook);
surface_stub!(napi_call_threadsafe_function);
surface_stub!(napi_create_bigint_uint64);
surface_stub!(napi_create_bigint_words);
surface_stub!(napi_create_threadsafe_function);
surface_stub!(napi_get_threadsafe_function_context);
surface_stub!(napi_get_uv_event_loop);
surface_stub!(napi_get_value_bigint_uint64);
surface_stub!(napi_get_value_bigint_words);
surface_stub!(napi_module_register);
surface_stub!(napi_ref_threadsafe_function);
surface_stub!(napi_release_threadsafe_function);
surface_stub!(napi_remove_async_cleanup_hook);
surface_stub!(napi_unref_threadsafe_function);

pub fn force_link_surface() -> usize {
    let fns: &[*const ()] = &[
        napi_acquire_threadsafe_function as *const (),
        napi_add_async_cleanup_hook as *const (),
        napi_call_threadsafe_function as *const (),
        napi_create_bigint_uint64 as *const (),
        napi_create_bigint_words as *const (),
        napi_create_threadsafe_function as *const (),
        napi_get_threadsafe_function_context as *const (),
        napi_get_uv_event_loop as *const (),
        napi_get_value_bigint_uint64 as *const (),
        napi_get_value_bigint_words as *const (),
        napi_module_register as *const (),
        napi_ref_threadsafe_function as *const (),
        napi_release_threadsafe_function as *const (),
        napi_remove_async_cleanup_hook as *const (),
        napi_unref_threadsafe_function as *const (),
    ];
    std::hint::black_box(fns.iter().map(|p| *p as usize).fold(0usize, usize::wrapping_add))
}
