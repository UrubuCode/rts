//! `alloc` namespace — alocacao raw via std::alloc.
//!
//! UNSAFE: caller eh responsavel por dealloc com mesmo size/align.
//! Para ergonomia preferir namespace `buffer` (vec u8 com handles GC).

pub mod abi;
pub mod ops;
