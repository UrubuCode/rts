//! `collections` namespace — HashMap<string, i64> e Vec<i64> via
//! HandleTable do gc.
//!
//! Escopo intencionalmente minimo: valores sao sempre i64. Caller
//! interpreta como inteiro, handle (string, bigfloat, etc) ou bool
//! conforme o uso. Quando object literals (#53) chegarem, um
//! `MapAny` com valores polimorficos sera natural.

pub mod abi;
pub mod map;
pub mod vec;
