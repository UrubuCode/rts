//! Print every Cranelift setting the linked version exposes, with its default.
//! `cargo run --release --example clif_flags -p rts-codegen-new 2>/dev/null`
//!
//! Exists so a startup-tuning decision is made against the ACTUAL flag surface of
//! the pinned Cranelift (0.131), not against remembered documentation — the
//! settings list changes between releases, and a `set()` whose Result is ignored
//! fails silently on a flag that no longer exists.
use cranelift_codegen::settings;

fn main() {
    let f = settings::Flags::new(settings::builder());
    println!("{f}");
}
