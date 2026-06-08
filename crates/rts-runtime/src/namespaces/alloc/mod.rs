//! `alloc` namespace — raw allocation via std::alloc.
//!
//! UNSAFE: caller eh responsavel por dealloc com mesmo size/align. Para
//! ergonomia preferir namespace `buffer` (vec u8 com handles GC).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc};

use rts_abi::ty::I64;
use rts_macro::rts_namespace;

fn make_layout(size: i64, align: i64) -> Option<Layout> {
    if size < 0 || align <= 0 {
        return None;
    }
    Layout::from_size_align(size as usize, align as usize).ok()
}

/// Allocator raw via std::alloc. UNSAFE — pareie alloc/dealloc com mesmo size/align.
#[rts_namespace(alloc)]
impl AllocNs {
    /// Aloca size bytes alinhados a `align`. Retorna ponteiro ou 0 em falha.
    #[rts_fn]
    pub fn alloc(size: I64, align: I64) -> I64 {
        let Some(layout) = make_layout(size, align) else {
            return 0;
        };
        if layout.size() == 0 {
            return 0;
        }
        unsafe { alloc(layout) as i64 }
    }

    /// Aloca size bytes zerados, alinhados a `align`.
    #[rts_fn]
    pub fn alloc_zeroed(size: I64, align: I64) -> I64 {
        let Some(layout) = make_layout(size, align) else {
            return 0;
        };
        if layout.size() == 0 {
            return 0;
        }
        unsafe { alloc_zeroed(layout) as i64 }
    }

    /// Libera ptr previamente alocado com mesmo size/align.
    #[rts_fn]
    pub fn dealloc(ptr: I64, size: I64, align: I64) {
        if ptr == 0 {
            return;
        }
        let Some(layout) = make_layout(size, align) else {
            return;
        };
        if layout.size() == 0 {
            return;
        }
        unsafe { dealloc(ptr as *mut u8, layout) };
    }

    /// Realoca ptr (size_old, align) para new_size. Retorna novo ptr ou 0.
    #[rts_fn(
        ts = "realloc(ptr: number, size_old: number, align: number, new_size: number): number"
    )]
    pub fn realloc(ptr: I64, size_old: I64, align: I64, new_size: I64) -> I64 {
        if ptr == 0 || new_size <= 0 {
            return 0;
        }
        let Some(layout) = make_layout(size_old, align) else {
            return 0;
        };
        unsafe { realloc(ptr as *mut u8, layout, new_size as usize) as i64 }
    }
}
