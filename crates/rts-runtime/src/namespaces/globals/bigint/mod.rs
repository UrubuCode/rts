//! `BigInt` global class — stub minimal (cross-runtime #742).
//!
//! RTS nao tem BigInt real (issue #219), tratamos como i64. Essa class
//! expoe os helpers staticos `asIntN(bits, n)` / `asUintN(bits, n)`
//! que sao usados em testes cross-runtime.

pub mod abi;
pub mod rt;

pub use abi::BIGINT_CLASS_SPEC;
