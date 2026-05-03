//! Array — namespace ABI + GlobalClassSpec para o tipo primitivo JS Array.
//!
//! Metodos estaticos: from(), isArray().
//! Metodos de instancia: length (getter), push(), pop(), shift(), unshift(),
//! indexOf(), includes(), join(), reverse(), slice(), concat(), flat(),
//! sort(), find(), findIndex(), every(), some(), map(), filter(), reduce(), forEach().
//! Implementacoes runtime em rt.rs.

pub mod abi;
pub mod rt;

