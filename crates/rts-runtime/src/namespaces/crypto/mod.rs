//! `crypto` namespace — primitivos criptograficos.
//!
//! Sem deps grandes: sha2 (ja no Cargo.toml), base64/hex implementados
//! inline, CSPRNG via OS API direta.

pub mod abi;
pub mod encode;
pub mod hash;
pub mod random;
