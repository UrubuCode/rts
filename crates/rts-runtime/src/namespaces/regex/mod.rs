//! `regex` namespace — expressoes regulares via crate `regex`.
//!
//! Compilacao retorna handle (Entry::Regex). Operacoes (test/find/replace)
//! aceitam o handle como primeiro argumento. Literais TS `/pat/flags`
//! sao desugared no codegen para `regex.compile(pat, flags)`.

pub mod abi;
pub mod ops;
