//! `date` namespace — primitivas pra implementar a API Date do JS.
//!
//! Modelo: cada Date e' um i64 com ms desde Unix epoch (UTC). Esse e' o
//! formato canonico de troca — getters e setters operam sobre ele. Sem
//! handles, sem alocacao: o Date e' o proprio i64.
//!
//! Conversoes pra calendario fields (year/month/day/hour/min/sec/ms)
//! usam algoritmo Howard Hinnant (chrono::civil_from_days), portado
//! manualmente pra evitar dependencia em chrono. Cobre 1970-9999 com
//! correcao de leap years.

pub mod abi;
pub mod ops;

pub use abi::SPEC;
