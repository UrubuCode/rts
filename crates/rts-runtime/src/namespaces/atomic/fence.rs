//! Memory fences — std::sync::atomic::fence.

use std::sync::atomic::{Ordering, fence};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_ACQUIRE() {
    fence(Ordering::Acquire);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_RELEASE() {
    fence(Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_SEQ_CST() {
    fence(Ordering::SeqCst);
}
