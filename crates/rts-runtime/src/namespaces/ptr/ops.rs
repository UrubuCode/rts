//! `ptr` runtime ops — raw pointer reads/writes/copy.
//!
//! Toda funcao eh inerentemente unsafe; encapsulamos com extern "C"
//! e o caller TS eh responsavel por garantir validade.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_NULL() -> i64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_IS_NULL(p: i64) -> i64 {
    if p == 0 { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_I64(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    unsafe { std::ptr::read_unaligned(p as *const i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_I32(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const i32) };
    v as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_U8(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const u8) };
    v as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_F64(p: i64) -> f64 {
    if p == 0 {
        return 0.0;
    }
    unsafe { std::ptr::read_unaligned(p as *const f64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_I64(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i64, value) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_I32(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i32, value as i32) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_U8(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut u8, value as u8) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_F64(p: i64, value: f64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut f64, value) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_COPY(dst: i64, src: i64, n: i64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy(src as *const u8, dst as *mut u8, n as usize) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_COPY_NONOVERLAPPING(dst: i64, src: i64, n: i64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n as usize) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_BYTES(dst: i64, value: i64, n: i64) {
    if dst == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::write_bytes(dst as *mut u8, value as u8, n as usize) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_OFFSET(p: i64, n: i64) -> i64 {
    p.wrapping_add(n)
}
