//! Side-table para `TemplateStringsArray.raw` (#744).
//!
//! Tagged templates passam `strings` como Vec dos cooked. O JS spec define
//! que esse mesmo objeto tem propriedade `.raw` com os valores raw (sem
//! escape interpretation). Aqui mantemos um mapa thread-local
//! `cooked_handle -> raw_vec_handle` consultado pelo codegen em
//! `member.raw` quando o `member` foi marcado como tagged-tpl arg.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static RAW_MAP: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
}

/// Associa `cooked_handle` (Vec) com `raw_handle` (Vec) no thread-local.
/// Chamado pelo codegen do tagged template apos lower do cooked array.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_TAGGED_RAW_REGISTER(cooked_handle: u64, raw_handle: u64) {
    RAW_MAP.with(|m| {
        m.borrow_mut().insert(cooked_handle, raw_handle);
    });
}

/// Retorna o `raw_handle` associado a `cooked_handle`, ou 0 se nao houver.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_TAGGED_RAW_GET(cooked_handle: u64) -> i64 {
    RAW_MAP.with(|m| m.borrow().get(&cooked_handle).copied().unwrap_or(0)) as i64
}
