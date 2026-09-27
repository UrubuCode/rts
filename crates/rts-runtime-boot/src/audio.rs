//! O host JIT registra `rts:audio` por `rts_audio::install`; o AOT reusa o
//! mesmo registro, como `physics.rs`.
use rts_core::entry::Context;

pub fn keep() -> usize {
    #[cfg(feature = "audio")]
    { (rts_audio::install as *const () as usize) & 1 }
    #[cfg(not(feature = "audio"))]
    { 0 }
}

pub fn install(context: &mut Context) {
    #[cfg(feature = "audio")]
    rts_audio::install(context);
    #[cfg(not(feature = "audio"))]
    let _ = context;
}

pub fn shutdown() {
    #[cfg(feature = "audio")]
    rts_audio::shutdown();
}
