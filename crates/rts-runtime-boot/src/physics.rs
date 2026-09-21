//! The JIT host already registers `rts:rigid` through rts_physics::install.
//! Reuse that registration for AOT, including archives without a compiler.
use rts_core::entry::Context;

pub fn keep() -> usize {
    #[cfg(feature = "physics")]
    { (rts_physics::install as usize) & 1 }
    #[cfg(not(feature = "physics"))]
    { 0 }
}

pub fn install(context: &mut Context) {
    #[cfg(feature = "physics")]
    rts_physics::install(context);
    #[cfg(not(feature = "physics"))]
    let _ = context;
}

// No second solver or registry: this is the same installation as the host.
