//! Superfície N-API restante (2 stubs):
//! - napi_get_uv_event_loop: precisa de shim libuv sobre tokio (#207)
//! - napi_module_register: registro LEGADO (não-N-API, fora de escopo)
//! Stubs napi_generic_failure p/ load_all() do napi-sys. Ver issue #1548.

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

surface_stub!(napi_get_uv_event_loop);
surface_stub!(napi_module_register);

pub fn force_link_surface() -> usize {
    let fns: &[*const ()] = &[
        napi_get_uv_event_loop as *const (),
        napi_module_register as *const (),
    ];
    std::hint::black_box(fns.iter().map(|p| *p as usize).fold(0usize, usize::wrapping_add))
}
