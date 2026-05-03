//! Object — namespace ABI + GlobalClassSpec para o tipo primitivo JS Object.
//!
//! Metodos estaticos: keys(), values(), entries(), hasOwn(), assign(),
//! create(), fromEntries(), defineProperty(), freeze(), seal().
//! Metodos de instancia: toString(), hasOwnProperty().
//! Implementacoes runtime em rt.rs.

pub mod abi;
pub mod rt;

