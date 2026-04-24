//! `hash` namespace — hash nao-criptografico (SipHash via DefaultHasher).
//!
//! Uso principal: chaves de HashMap proprio, deduplicacao, checksums
//! rapidos. **Nao use para seguranca** — SipHash resiste a HashDoS mas
//! nao e pre-image resistente. Para SHA/BLAKE veja namespace `crypto`
//! (#24).

pub mod abi;
pub mod ops;
