//! Slice-1 unit test for step 5b: `__rtsadp_ta_view_base_len` resolves a level-B
//! view to a correct (base, count, elem_bytes), and a non-view returns 0.
unsafe extern "C" {
    fn __RTS_FN_NS_BUFFER_ALLOC_ZEROED(size: i64) -> u64;
    fn __RTS_FN_GL_TA_NEW_U8(arg: u64) -> u64;
    fn __RTS_FN_GL_TA_NEW_U32(arg: u64) -> u64;
    fn __RTS_FN_GL_TA_SET_ELEM(handle: u64, index: i64, elem_bytes: i64, is_float: i64, val: i64);
    fn __rtsadp_ta_view_base_len(view_word: u64, out_base: *mut i64, out_count: *mut i64) -> i64;
    fn __RTS_FN_NS_GC_POLY_FROM_HANDLE(handle: u64) -> u64;
}

fn view_word_over(buf_bytes: i64, ctor: unsafe extern "C" fn(u64) -> u64) -> (u64, u64) {
    let bh = unsafe { __RTS_FN_NS_BUFFER_ALLOC_ZEROED(buf_bytes) };
    let bw = rts_adapters::value::PolyValue::from_object_handle(unsafe {
        __RTS_FN_NS_GC_POLY_FROM_HANDLE(bh)
    })
    .raw();
    let vraw = unsafe { ctor(bw) };
    let vw = rts_adapters::value::PolyValue::from_object_handle(unsafe {
        __RTS_FN_NS_GC_POLY_FROM_HANDLE(vraw)
    })
    .raw();
    (bh, vw)
}

#[test]
fn base_len_u8() {
    let (bh, vw) = view_word_over(16, __RTS_FN_GL_TA_NEW_U8);
    // Write a known byte through the buffer so we can verify the base points at it.
    unsafe { __RTS_FN_GL_TA_SET_ELEM(bh, 3, 1, 0, 0x7a) };
    let (mut base, mut count) = (0i64, 0i64);
    let ebytes = unsafe { __rtsadp_ta_view_base_len(vw, &mut base, &mut count) };
    assert_eq!(ebytes, 1, "u8 elem bytes");
    assert_eq!(count, 16, "u8 count = 16 bytes / 1");
    assert_ne!(base, 0, "base non-null");
    let byte3 = unsafe { *((base as *const u8).add(3)) };
    assert_eq!(byte3, 0x7a, "base+3 reads the byte written via TA_SET_ELEM");
}

#[test]
fn base_len_u32() {
    let (_bh, vw) = view_word_over(16, __RTS_FN_GL_TA_NEW_U32);
    let (mut base, mut count) = (0i64, 0i64);
    let ebytes = unsafe { __rtsadp_ta_view_base_len(vw, &mut base, &mut count) };
    assert_eq!(ebytes, 4, "u32 elem bytes");
    assert_eq!(count, 4, "u32 count = 16 bytes / 4");
    assert_ne!(base, 0);
}

#[test]
fn non_view_returns_zero() {
    let (mut base, mut count) = (7i64, 7i64);
    let r = unsafe { __rtsadp_ta_view_base_len(0, &mut base, &mut count) };
    assert_eq!(r, 0);
    assert_eq!(base, 0);
    assert_eq!(count, 0);
}
