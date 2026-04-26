//! `thread` namespace — primitivas de threads baseadas em
//! `std::thread`.
//!
//! `spawn` recebe um ponteiro de funcao `extern "C" fn(u64) -> u64` e um
//! argumento `u64` (handle ou inteiro). A thread retorna um `u64` que e
//! coletado por `join`. `JoinHandle<u64>` vive em `Box` dentro da
//! `HandleTable` para estabilizar enderecos enquanto o slot existir.
//!
//! `id()` retorna um id `u64` estavel por thread (atribuido na primeira
//! chamada via contador atomico — o `ThreadId::as_u64` ainda e unstable
//! em Rust 1.93).

pub mod abi;
pub mod info;
pub mod join;
pub mod spawn;
