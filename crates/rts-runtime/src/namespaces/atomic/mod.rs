//! `atomic` namespace — primitivas atomicas baseadas em
//! `std::sync::atomic` (AtomicI64, AtomicBool, fence).
//!
//! Toda operacao usa `Ordering::SeqCst` para uma semantica simples e
//! previsivel. Handles armazenam `Box<AtomicI64>` / `Box<AtomicBool>`
//! para estabilizar o endereco enquanto o slot da `HandleTable` viver.

pub mod abi;
pub mod bool;
pub mod fence;
pub mod float;
pub mod int;
