//! Object — GlobalClassSpec para o tipo primitivo JS Object.
//!
//! Cobre métodos estáticos como `Object.create`, `Object.keys`, 
//! `Object.values`, `Object.entries`, `Object.assign`, `Object.freeze`,
//! `Object.fromEntries`, e métodos de instância como `hasOwnProperty`.
//!
//! A implementação usa internamente o namespace `collections` (IndexMap)
//! para armazenar propriedades de objetos.

pub mod abi;
pub mod rt;
