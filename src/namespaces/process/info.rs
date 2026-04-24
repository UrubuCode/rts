//! Info estatico do processo corrente.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_PID() -> i64 {
    std::process::id() as i64
}
