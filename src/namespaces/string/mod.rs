//! `string` namespace — operacoes ricas sobre strings UTF-8.
//!
//! Complementa o pool de strings do `gc` (alocacao + concat +
//! conversao basica) com metodos idiomaticos: busca, transformacao,
//! trim, replace, contagem.

pub mod abi;
pub mod replace;
pub mod search;
pub mod split;
pub mod transform;
