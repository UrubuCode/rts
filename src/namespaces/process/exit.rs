//! Termina o processo com codigo ou via abort/sinal.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_EXIT(code: i32) {
    std::process::exit(code)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ABORT() {
    std::process::abort()
}
