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
/// `RTS_COLD_BLOCKS=0` — stop marking the post-call error edge as a cold block
/// (`FunctionBuilder::set_cold_block`). Layout-only, so this is an A/B switch for
/// what the hint is worth, not a correctness fallback.
pub(super) fn cold_blocks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_COLD_BLOCKS")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// `RTS_INT_OVERFLOW=1` — overflow-check the proven-int `+`/`-`/`*` path
/// (`sadd_overflow`/`ssub_overflow`/`smul_overflow`) and promote to `f64` on
/// overflow, which is what JS-number semantics require. OFF by default, and the
/// reason is measured rather than assumed:
///
/// * Correctness with it ON is real. `4611686018427387904 + 4611686018427387904`
///   prints `9223372036854776000` (Node's answer) instead of the wrapped
///   `-9223372036854776000`, and `* 4` prints `18446744073709552000` instead of `0`.
/// * Cost with it ON is also real: an int-heavy 50M-iteration loop goes from
///   ~34 ms to ~224 ms (6.6x) when the two edges merge as `Float64`, and to
///   ~735 ms (21x) when they merge as `Tagged`.
///
/// What blocks turning it on: it cannot be applied selectively today. RTS has
/// BOTH semantics — a JS `number` is a double (wrapping is wrong), while a value
/// declared `i64`/`u32` is a native fixed-width integer (wrapping is the declared
/// contract). `rts-hir` types EVERY integral literal as `HirType::I64` regardless
/// of the annotation (`lower.rs:915`/`:927`), so the lowering cannot tell the two
/// apart, and a blanket check taxes the native-int path for a JS rule that does
/// not govern it. Distinguishing them needs an "annotated" bit on the HIR binding
/// (~95 construction sites), which is the follow-up this flag is waiting on.
pub(super) fn int_overflow_checks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_INT_OVERFLOW")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

pub(super) fn verifier_forced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_CLIF_VERIFIER")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}
