//! Array — GlobalClassSpec para o tipo primitivo JS Array.
//!
//! Cobre métodos estáticos como `Array.isArray`, `Array.from`,
//! e métodos de instância como `push`, `pop`, `map`, `filter`, etc.
//!
//! A implementação usa internamente o namespace `collections` (Vec)
//! para armazenar elementos do array.

pub mod abi;
pub mod rt;
