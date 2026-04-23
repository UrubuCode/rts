//! RTS binary entry point.
//!
//! The binary owns the `include_bytes!` of the RTS static library: keeping
//! the embed out of the library crate avoids the staticlib-including-itself
//! recursion that would otherwise occur (`target/*.lib` is what `rts` emits
//! AND what the binary needs to embed for user builds).

use anyhow::Result;

use rts::runtime::embedded::{EmbeddedRuntime, install};

static RTS_RUNTIME_BYTES: &[u8] = include_bytes!(env!("RTS_RUNTIME_STATICLIB"));
static RTS_RUNTIME_EXT: &str = env!("RTS_RUNTIME_STATICLIB_EXT");

fn main() -> Result<()> {
    install(EmbeddedRuntime {
        bytes: RTS_RUNTIME_BYTES,
        extension: RTS_RUNTIME_EXT,
    });
    rts::cli::dispatch(std::env::args())
}
