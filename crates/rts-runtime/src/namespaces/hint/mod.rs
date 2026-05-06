//! `hint` namespace — performance hints (std::hint).
//!
//! - spin_loop: hint para spin-wait loops (PAUSE em x86, YIELD em ARM).
//! - black_box: opaque pra otimizador (impede dead-code elimination).

pub mod abi;
pub mod ops;
