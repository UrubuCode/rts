//! The default archive an AOT-compiled program links against — `rts compile`
//! with no flag.
//!
//! # Why it holds no code of its own
//!
//! Because a facade that implements anything is a second place to look for
//! it. `rts-runtime-boot::run` is the whole startup sequence — install stack
//! scanning, seed the tables, install `rts-std`/`rts-node`/`rts:dom`/`rts:egui`,
//! call the compiled entry, drain the event loop — and this crate's only job
//! is to give it the platform's C ABI under the name a linker looks for.
//!
//! # Why the sequence itself lives in a THIRD crate
//!
//! `rts compile --embed-compiler` needs the exact same sequence plus one
//! extra registration (`rts_host::install_compiler`), and reusing it by
//! having `rts-runtime-jit` depend on THIS crate was tried first — it
//! compiled and linked without error, and silently ran the wrong `main`.
//! `rts-runtime-boot`'s own module doc has the measured cause and why a
//! third crate, rather than a smaller fix, is what closes it: neither this
//! crate nor `rts-runtime-jit` may depend on the other, because a
//! `#[unsafe(no_mangle)]` item — `main` is one — is bundled into a
//! dependent's `staticlib` unconditionally once the dependency is reached at
//! all, regardless of whether the dependent's own code calls that
//! particular item.
#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const i8) -> i32 {
    rts_runtime_boot::run(argc, argv, None)
}
