//! Superfície N-API restante (1 stub): napi_module_register é o registro
//! LEGADO (node_module_register, não-N-API, acoplado a V8) — fora de escopo
//! por design (addons N-API usam napi_register_module_v1). Stub
//! napi_generic_failure p/ load_all() do napi-sys. Ver issue #1548.

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

surface_stub!(napi_module_register);

pub fn force_link_surface() -> usize {
    let fns: &[*const ()] = &[
        napi_module_register as *const (),
    ];
    std::hint::black_box(fns.iter().map(|p| *p as usize).fold(0usize, usize::wrapping_add))
}
