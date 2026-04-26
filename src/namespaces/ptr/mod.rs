//! `ptr` namespace — operacoes raw sobre ponteiros (std::ptr).
//!
//! Toda funcao eh `unsafe` por natureza — caller eh responsavel por
//! validez/alinhamento/lifetime. Use com `buffer.ptr(handle)` para
//! ler/escrever buffers.

pub mod abi;
pub mod ops;
