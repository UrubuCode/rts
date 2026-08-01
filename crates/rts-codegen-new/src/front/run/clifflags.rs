//! Cranelift ISA-FLAG switches shared by both module paths (JIT and AOT), and
//! the env escape hatches that make their cost measurable in one binary.

/// `RTS_CLIF_VERIFIER=1` — force Cranelift's IR verifier ON in a RELEASE binary,
/// on both the JIT ([`super::module_jit::make_module`]) and the AOT
/// ([`super::module_aot::make_object_module`]) path. Release builds disable it
/// (it runs several times per function and taxes every startup); debug builds
/// keep it, where a malformed lowering must be caught loudly.
///
/// This exists so the verifier's cost is a MEASUREMENT rather than an
/// attribution — `CRANELIFT_IMPLEMENTATION.md` §4 read a 2x JIT/AOT
/// machine-compile gap as "consistent with the verifier", with two variables
/// differing. A/B'd with this switch it is 4.8 ms of the AOT phase (36.4 -> 41.2
/// ms) and 5.5 ms of the JIT phase; the remaining AOT/JIT gap is the AOT path's
/// extra IR (string literals as data objects + a `string_from_static` call
/// instead of the JIT's compile-time-baked handle), not the verifier.
pub(super) fn verifier_forced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_CLIF_VERIFIER")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}
