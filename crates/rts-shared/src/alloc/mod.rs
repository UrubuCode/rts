//! `alloc` namespace — raw allocation via std::alloc.
//!
//! UNSAFE: caller eh responsavel por dealloc com mesmo size/align. Para
//! ergonomia preferir namespace `buffer` (vec u8 com handles GC).
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc};

use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, sig};

fn make_layout(size: i64, align: i64) -> Option<Layout> {
    if size < 0 || align <= 0 {
        return None;
    }
    Layout::from_size_align(size as usize, align as usize).ok()
}

/// Aloca size bytes alinhados a `align`. Retorna ponteiro ou 0 em falha.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ALLOC_ALLOC(size: i64, align: i64) -> i64 {
    let Some(layout) = make_layout(size, align) else {
        return 0;
    };
    if layout.size() == 0 {
        return 0;
    }
    unsafe { alloc(layout) as i64 }
}

/// Aloca size bytes zerados, alinhados a `align`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ALLOC_ALLOC_ZEROED(size: i64, align: i64) -> i64 {
    let Some(layout) = make_layout(size, align) else {
        return 0;
    };
    if layout.size() == 0 {
        return 0;
    }
    unsafe { alloc_zeroed(layout) as i64 }
}

/// Libera ptr previamente alocado com mesmo size/align.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ALLOC_DEALLOC(ptr: i64, size: i64, align: i64) {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ALLOC_REALLOC(
    ptr: i64,
    size_old: i64,
    align: i64,
    new_size: i64,
) -> i64 {
    if ptr == 0 || new_size <= 0 {
        return 0;
    }
    let Some(layout) = make_layout(size_old, align) else {
        return 0;
    };
    unsafe { realloc(ptr as *mut u8, layout, new_size as usize) as i64 }
}

fn func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
        emit: None,
    }
}

/// Registra a namespace `alloc` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("alloc")
        .doc("Allocator raw via std::alloc. UNSAFE — pareie alloc/dealloc com mesmo size/align.")
        .member(func(
            "alloc",
            "__RTS_FN_NS_ALLOC_ALLOC",
            sig!(I64, I64 => I64),
            "alloc(size: number, align: number): number",
            "Aloca size bytes alinhados a `align`. Retorna ponteiro ou 0 em falha.",
            __RTS_FN_NS_ALLOC_ALLOC as *const u8,
        ))
        .member(func(
            "alloc_zeroed",
            "__RTS_FN_NS_ALLOC_ALLOC_ZEROED",
            sig!(I64, I64 => I64),
            "alloc_zeroed(size: number, align: number): number",
            "Aloca size bytes zerados, alinhados a `align`.",
            __RTS_FN_NS_ALLOC_ALLOC_ZEROED as *const u8,
        ))
        .member(func(
            "dealloc",
            "__RTS_FN_NS_ALLOC_DEALLOC",
            sig!(I64, I64, I64 => Void),
            "dealloc(ptr: number, size: number, align: number): void",
            "Libera ptr previamente alocado com mesmo size/align.",
            __RTS_FN_NS_ALLOC_DEALLOC as *const u8,
        ))
        .member(func(
            "realloc",
            "__RTS_FN_NS_ALLOC_REALLOC",
            sig!(I64, I64, I64, I64 => I64),
            "realloc(ptr: number, size_old: number, align: number, new_size: number): number",
            "Realoca ptr (size_old, align) para new_size. Retorna novo ptr ou 0.",
            __RTS_FN_NS_ALLOC_REALLOC as *const u8,
        ))
        .done();
}
