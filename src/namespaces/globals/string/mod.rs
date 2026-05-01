//! String — namespace ABI + GlobalClassSpec para o tipo primitivo JS String.
//!
//! Metodos de namespace (rts:string) em search/transform/replace/split.
//! Metodos de instancia JS (str.slice, str.split, etc.) em rt.rs.

pub mod abi;
pub mod replace;
pub mod rt;
pub mod search;
pub mod split;
pub mod transform;
