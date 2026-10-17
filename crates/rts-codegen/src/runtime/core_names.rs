//! The one table that binds a name imported from `"rts:core"` to a neutral
//! [`CorePrimitive`], shared by both emitters (`emit/core_call.rs` for the
//! running emitter, `lower/intrinsic.rs::core_intrinsic` for the MIR stage) so
//! they cannot disagree about which names exist.
//!
//! `rts:core` is NOT a runtime module: an `import { imul } from "rts:core"` is
//! compile-time sugar. The import emits nothing, and every call of the bound
//! name lowers to the direct op for the primitive.
//!
//! Scaffolding: `imul` duplicates `Math.imul`. The point is the mechanism, which
//! later (unsafe) primitives reuse.
//!
//! Each entry carries `public`, from `CorePrimitive::is_public`. The path gate
//! exists: `emit/module.rs` refuses a non-public entry unless `ctx.origin` is
//! `Origin::Std` (alloc/slotLoad/slotStore are the unsafe ones, reserved here).

use rts_corelib::CorePrimitive;

/// The module specifier whose imports are resolved at compile time.
pub const SPECIFIER: &str = "rts:core";

/// What a name binds.
#[derive(Clone, Copy, Debug)]
pub struct CoreName {
    /// The neutral primitive.
    pub primitive: CorePrimitive,
    /// Written arguments.
    pub arity: usize,
    /// Whether any module may use it (`CorePrimitive::is_public`).
    pub public: bool,
}

/// The second specifier of the same mechanism: the NATIVE-HOST layer. Its names
/// bind to host entries that read OS state, and exist only on a native target
/// (a browser has no OS), unlike the universal `rts:core`.
pub const SYS_SPECIFIER: &str = "rts:sys";

/// Whether `specifier` is resolved at compile time by this table.
pub fn is_compile_time(specifier: &str) -> bool {
    specifier == SPECIFIER || specifier == SYS_SPECIFIER
}

/// What an imported local name is bound to: one mechanism, two specifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// A neutral `rts:core` primitive.
    Core(CorePrimitive),
    /// A host entry from `rts:sys`.
    Sys(Sys),
}

/// A `rts:sys` host query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sys {
    /// Physical memory installed, in bytes.
    TotalMem,
    /// Physical memory available, in bytes.
    FreeMem,
    /// Seconds since boot.
    Uptime,
    /// Logical CPU count.
    CpuCount,
    /// `"win32" | "linux" | "darwin"`: a compile-time string constant.
    Platform,
    /// `"x64" | "arm64" | ...`: a compile-time string constant.
    Arch,
    /// `"LE" | "BE"`: a compile-time string constant.
    Endianness,
    /// `os.hostname()`: a runtime string, from `RuntimeOp::SysText`.
    Hostname,
    /// `os.homedir()`: a runtime string.
    Homedir,
    /// `os.tmpdir()`: a runtime string.
    Tmpdir,
    /// `process.cwd()`: a runtime string.
    Cwd,
}

impl Sys {
    /// Whether the answer is a runtime STRING (`RuntimeOp::SysText`) rather than
    /// a number (`RuntimeOp::SysMemory`).
    pub fn is_text(self) -> bool {
        matches!(self, Sys::Hostname | Sys::Homedir | Sys::Tmpdir | Sys::Cwd)
    }

    /// The selector `RuntimeOp::SysMemory` (or `SysText`, see [`Sys::is_text`])
    /// takes. Meaningless for a constant.
    pub fn selector(self) -> i64 {
        match self {
            Sys::TotalMem => 0,
            Sys::FreeMem => 1,
            Sys::Uptime => 2,
            Sys::CpuCount => 3,
            Sys::Hostname => 0,
            Sys::Homedir => 1,
            Sys::Tmpdir => 2,
            Sys::Cwd => 3,
            Sys::Platform | Sys::Arch | Sys::Endianness => unreachable!("a constant has no entry"),
        }
    }

    /// The text of a query known at compile time, or `None` for one that needs
    /// the entry. Node's `os.platform()/arch()/endianness()` spellings.
    ///
    /// TODO: like [`host_available`], this reads the COMPILER's own build
    /// target, which is the compilation target today (only a native backend
    /// exists). Thread the real target through when a non-host backend lands.
    pub fn constant(self) -> Option<&'static str> {
        match self {
            Sys::Platform => Some(if cfg!(target_os = "windows") {
                "win32"
            } else if cfg!(target_os = "macos") {
                "darwin"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else {
                std::env::consts::OS
            }),
            Sys::Arch => Some(if cfg!(target_arch = "x86_64") {
                "x64"
            } else if cfg!(target_arch = "aarch64") {
                "arm64"
            } else if cfg!(target_arch = "x86") {
                "ia32"
            } else {
                std::env::consts::ARCH
            }),
            Sys::Endianness => Some(if cfg!(target_endian = "little") { "LE" } else { "BE" }),
            _ => None,
        }
    }

    /// Written arguments.
    pub fn arity(self) -> usize {
        0
    }
}

/// Whether the target being compiled for has a host (an operating system).
///
/// TODO: the codegen has no target concept yet; this reads the architecture the
/// COMPILER was built for, which is the target today because there is only a
/// native backend. When a wasm backend lands, thread the real target here --
/// `rts:sys` must be refused for it.
pub fn host_available() -> bool {
    !cfg!(target_arch = "wasm32")
}

/// The host query a `rts:sys` export name means, or `None` for an unknown name.
pub fn lookup_sys(name: &str) -> Option<Sys> {
    match name {
        "totalmem" => Some(Sys::TotalMem),
        "freemem" => Some(Sys::FreeMem),
        "uptime" => Some(Sys::Uptime),
        "cpuCount" => Some(Sys::CpuCount),
        "platform" => Some(Sys::Platform),
        "arch" => Some(Sys::Arch),
        "endianness" => Some(Sys::Endianness),
        "hostname" => Some(Sys::Hostname),
        "homedir" => Some(Sys::Homedir),
        "tmpdir" => Some(Sys::Tmpdir),
        "cwd" => Some(Sys::Cwd),
        _ => None,
    }
}

/// The primitive `@intrinsic("name")` means: the SAME table and the same lowercase
/// import spelling as `lookup` ("sqrt", not "Sqrt"), so the decorator and the
/// `rts:core` import cannot disagree about which ops exist or their arity.
pub fn intrinsic_lookup(name: &str) -> Option<CoreName> {
    lookup(name)
}

/// The primitive a `rts:core` export name means, or `None` for an unknown name.
pub fn lookup(name: &str) -> Option<CoreName> {
    let (primitive, arity) = match name {
        // UNSAFE (raw memory): reserved so the std-only gate is non-vacuous. No
        // lowering yet, so a CALL still hits the "no lowering" refusal; arity is a
        // placeholder.
        "alloc" => (CorePrimitive::Alloc, 1),
        "slotLoad" => (CorePrimitive::SlotLoad, 2),
        "slotStore" => (CorePrimitive::SlotStore, 3),
        "imul" => (CorePrimitive::I32Mul, 2),
        "clz32" => (CorePrimitive::Clz32, 1),
        "fround" => (CorePrimitive::Fround, 1),
        "sqrt" => (CorePrimitive::Sqrt, 1),
        "floor" => (CorePrimitive::Floor, 1),
        "ceil" => (CorePrimitive::Ceil, 1),
        "trunc" => (CorePrimitive::Trunc, 1),
        "abs" => (CorePrimitive::Abs, 1),
        "sign" => (CorePrimitive::Sign, 1),
        "min" => (CorePrimitive::Min, 2),
        "max" => (CorePrimitive::Max, 2),
        "round" => (CorePrimitive::Round, 1),
        "cbrt" => (CorePrimitive::Cbrt, 1),
        "exp" => (CorePrimitive::Exp, 1),
        "expm1" => (CorePrimitive::Expm1, 1),
        "log" => (CorePrimitive::Log, 1),
        "log2" => (CorePrimitive::Log2, 1),
        "log10" => (CorePrimitive::Log10, 1),
        "log1p" => (CorePrimitive::Log1p, 1),
        "sin" => (CorePrimitive::Sin, 1),
        "cos" => (CorePrimitive::Cos, 1),
        "tan" => (CorePrimitive::Tan, 1),
        "asin" => (CorePrimitive::Asin, 1),
        "acos" => (CorePrimitive::Acos, 1),
        "atan" => (CorePrimitive::Atan, 1),
        "sinh" => (CorePrimitive::Sinh, 1),
        "cosh" => (CorePrimitive::Cosh, 1),
        "tanh" => (CorePrimitive::Tanh, 1),
        "asinh" => (CorePrimitive::Asinh, 1),
        "acosh" => (CorePrimitive::Acosh, 1),
        "atanh" => (CorePrimitive::Atanh, 1),
        "pow" => (CorePrimitive::Pow, 2),
        "atan2" => (CorePrimitive::Atan2, 2),
        _ => return None,
    };
    Some(CoreName {
        primitive,
        arity,
        public: primitive.is_public(),
    })
}
