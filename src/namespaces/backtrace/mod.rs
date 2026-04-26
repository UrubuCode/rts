//! `backtrace` namespace — captura de stack trace via std::backtrace.
//!
//! Backtraces sao armazenadas no HandleTable como Entry::Backtrace.
//! to_string formata em string handle. is_enabled checa se
//! RUST_BACKTRACE esta set.

pub mod abi;
pub mod ops;
