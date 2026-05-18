//! `Headers` global class (#289).
//!
//! Multimap case-insensitive de header name -> lista de valores. Implementa
//! ctor (vazio ou array de pares), append, get (junta com ", "), set, has,
//! delete, entries, keys, values, getSetCookie (lista raw de set-cookie).

pub mod abi;
pub mod instance;
