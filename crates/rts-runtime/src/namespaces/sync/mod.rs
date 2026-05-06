//! `sync` namespace — primitivas de sincronizacao baseadas em
//! `std::sync` (Mutex, RwLock, Once).
//!
//! Mutex e RwLock guardam um valor i64 interno (handle ou inteiro) — o
//! caller TS-side e responsavel por chamar lock/unlock corretamente.
//!
//! Mutex/RwLock guards atravessam chamadas extern "C" via mapa thread-
//! local: `lock`/`read`/`write` armazenam o guard `'static` (alongado por
//! Box que vive enquanto o handle existe), e `unlock` o remove e dropa.

pub mod abi;
pub mod mutex;
pub mod once;
pub mod rwlock;
