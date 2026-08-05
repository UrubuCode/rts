//! Where things live.
//!
//! A slot table, addressed by index. The machine's 48-bit payload is a slot
//! index rather than an address, and every property that follows from that —
//! conservative scanning being safe, growth invalidating nothing, a moving
//! collector being possible later — is a consequence of the indirection rather
//! than of anything clever.

mod aside;
mod region;
mod slab;

pub use aside::Aside;
pub use region::{INLINE_SLOTS, Region, STRIDE};
pub use slab::{Handle, Slab, Slot, Stale};
