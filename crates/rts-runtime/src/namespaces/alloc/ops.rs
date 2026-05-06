//! `alloc` runtime — wraps std::alloc::{alloc, alloc_zeroed, dealloc, realloc}.

use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc};

fn make_layout(size: i64, align: i64) -> Option<Layout> {
    if size < 0 || align <= 0 {
        return None;
    }
    Layout::from_size_align(size as usize, align as usize).ok()
}

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
