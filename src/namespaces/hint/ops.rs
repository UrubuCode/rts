//! `hint` runtime operations.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_SPIN_LOOP() {
    std::hint::spin_loop();
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_BLACK_BOX_I64(value: i64) -> i64 {
    std::hint::black_box(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_BLACK_BOX_F64(value: f64) -> f64 {
    std::hint::black_box(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_UNREACHABLE() {
    // Em debug, aborta com mensagem clara. Em release, eh UB —
    // mas exposto como function call portavel via panic.
    if cfg!(debug_assertions) {
        panic!("hint.unreachable() atingido");
    }
    // SAFETY: caller garantiu que esta linha nao executa.
    unsafe { std::hint::unreachable_unchecked() }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_ASSERT_UNCHECKED(cond: i8) {
    if cfg!(debug_assertions) {
        if cond == 0 {
            panic!("hint.assert_unchecked: cond=false");
        }
        return;
    }
    // SAFETY: caller garantiu cond verdadeira.
    unsafe { std::hint::assert_unchecked(cond != 0) }
}
