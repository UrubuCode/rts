//! The one startup sequence, callable from a linked-together `.exe` instead of
//! from `rts-host`'s own process.
//!
//! # Why the entry point lives here and not synthesized into the object
//!
//! `rts-host::run::run_region` already IS this sequence, written in Rust,
//! with an order that is load-bearing — install stack scanning, seed the key
//! table, the literal table, the template table, the frame table, the
//! function-name table, install `rts-std` and `rts-node`, call the
//! entry, drain the event loop, report an uncaught throw. Synthesizing IR that
//! reproduced that order would be a second copy of it, and a second copy is how
//! the two drift — which here would show up as a miscompile, not an error,
//! because nothing checks that two hand-written orderings agree.
//!
//! So the object built by `rts_host::object::compile_to_object` exports its
//! script under the fixed name `__rts_script`, under the exact ABI convention
//! every compiled function uses, and this module supplies the process's actual
//! `main`: it runs the same sequence `run_region` runs, then calls that symbol.
//! One startup, two callers — this file's job is to be the second, not to
//! invent a third.
//!
//! # The region base — the part this had to get exactly right
//!
//! `rts_cranelift::mem::RegionBase::Symbol("__rts_region_base")` is what the
//! object's compiled code reads its heap base FROM: an object file cannot bake
//! in an address that does not exist until this binary runs, so the object
//! leaves the symbol undefined and this crate DEFINES it, as a plain data cell.
//!
//! [`REGION_BASE`] is written in [`start`], immediately after the region is
//! allocated and before anything else touches it — in particular before
//! `rts_std::install`, `rts_node::install`, or a single call into
//! compiled code, every one of which can allocate. If the write happened after
//! any of those, an address computed from the OLD (zero) value would already be
//! baked into a value or a cache entry, and it would stay wrong silently: the
//! arithmetic that turns a cell index into an address does not know the base it
//! used was never set.

use std::time::Duration;

use rts_core::entry::Context;
use rts_core::heap::Region;
use rts_core::value::Singletons;

/// The cell `RegionBase::Symbol("__rts_region_base")` names.
///
/// A plain `u64`, not an `AtomicU64`: this binary runs the program on exactly
/// one thread today (`compile_to_object` never asks for more than one region),
/// so there is exactly one writer, once, before compiled code's first read.
///
/// `#[unsafe(export_name = ...)]` rather than `#[unsafe(no_mangle)]` because the
/// name has to match [`rts_host`]'s `object::REGION_BASE_SYMBOL` exactly —
/// the same reasoning `#[rtse::entry]` states for every runtime entry point.
#[unsafe(export_name = "__rts_region_base")]
static mut REGION_BASE: u64 = 0;

unsafe extern "C" {
    /// The compiled program's entry, exported by the object under the fixed
    /// name `rts_host::object` places every program's script under.
    #[link_name = "__rts_script"]
    fn __rts_script(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> u64;
}

/// The top of the CURRENT thread's stack, on the one platform this is sound on.
///
/// Duplicated from `rts-host::stack` rather than shared: that crate is not
/// a dependency of this one (naming it would be backwards — the facade is what
/// an AOT binary links against, and `rts-host` is a JIT host that itself
/// depends on nothing this crate produces), and the function is a direct,
/// minimal use of one Win32 API rather than a decision either crate should be
/// the one place that states.
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
fn current_thread_stack_high() -> Option<usize> {
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let mut low: usize = 0;
    let mut high: usize = 0;
    // SAFETY: an ordinary Win32 call, out parameters only.
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    Some(high)
}

/// The honest answer off Windows: nobody has wired a sound call yet. See
/// `rts-host::stack`'s own doc comment — the same absence, for the same
/// reason.
#[cfg(not(all(target_arch = "x86_64", target_os = "windows")))]
fn current_thread_stack_high() -> Option<usize> {
    None
}

/// What `rts_host::object::write_manifest` wrote, read back.
///
/// Owns its bytes decoded into owned tables rather than borrowing the file's
/// buffer: `declare_keys`/`declare_literals` want slices of owned entries, and
/// a manifest that borrowed would tie the seed tables' lifetime to a buffer
/// this function has no reason to keep alive past seeding.
struct Manifest {
    singletons: [u32; 3],
    kinds: [u8; 2],
    keys: Vec<String>,
    /// Code units, not text. `declare_literals` says why: a string literal may
    /// be a lone surrogate, which no UTF-8 spelling can carry — and an AOT
    /// binary that lost one where a JIT run kept it would answer differently
    /// about the same program.
    literals: Vec<Vec<u16>>,
    templates: Vec<Vec<u32>>,
}

/// Reads a manifest written in `rts_host::object::write_manifest`'s format.
///
/// `None` on anything short of a well-formed file: a truncated read is not a
/// number this program is allowed to guess about, and [`start`] treats a
/// missing manifest as a reason to abort loudly rather than run with an empty
/// key table that would make every property access read as absent.
fn read_manifest(bytes: &[u8]) -> Option<Manifest> {
    let mut at = 0usize;
    let u32_at = |at: &mut usize| -> Option<u32> {
        let slice = bytes.get(*at..*at + 4)?;
        *at += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    let singletons = [
        u32_at(&mut at)?,
        u32_at(&mut at)?,
        u32_at(&mut at)?,
    ];
    let kinds = [*bytes.get(at)?, *bytes.get(at + 1)?];
    at += 4;

    let read_strings = |at: &mut usize| -> Option<Vec<String>> {
        let count = u32_at(at)?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let len = u32_at(at)? as usize;
            let slice = bytes.get(*at..*at + len)?;
            *at += len;
            out.push(String::from_utf8(slice.to_vec()).ok()?);
        }
        Some(out)
    };
    let read_units = |at: &mut usize| -> Option<Vec<Vec<u16>>> {
        let count = u32_at(at)?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let len = u32_at(at)? as usize;
            let mut units = Vec::with_capacity(len);
            for _ in 0..len {
                let slice = bytes.get(*at..*at + 2)?;
                *at += 2;
                units.push(u16::from_le_bytes(slice.try_into().ok()?));
            }
            out.push(units);
        }
        Some(out)
    };
    let keys = read_strings(&mut at)?;
    let literals = read_units(&mut at)?;

    let count = u32_at(&mut at)?;
    let mut templates = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let len = u32_at(&mut at)? as usize;
        let mut site = Vec::with_capacity(len);
        for _ in 0..len {
            site.push(u32_at(&mut at)?);
        }
        templates.push(site);
    }

    Some(Manifest {
        singletons,
        kinds,
        keys,
        literals,
        templates,
    })
}

/// Cells the AOT binary allocates, one per program — one region, same as
/// `rts_host::run::compile`'s default.
const CELLS: u32 = 1 << 16;

/// The process's real entry point, for a binary the object file's
/// `__rts_script` was linked into.
///
/// # Safety
///
/// Called by the C runtime with the platform's own `argc`/`argv` convention.
/// Nothing here reads them; the manifest is found next to the running
/// executable rather than on the command line.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const i8) -> i32 {
    // The sidecar file: `<exe>` with its extension replaced by `.rtsdata`. See
    // `rts_host::object`'s module doc for why this exists instead of a
    // second, linker-relocation-shaped mechanism.
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("rts: could not find this executable's own path: {error}");
            return 1;
        }
    };
    let manifest_path = exe.with_extension("rtsdata");
    let bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "rts: missing program data '{}': {error} — an AOT binary from `rts \
                 compile` is not standalone of this file; moving the .exe without it \
                 breaks property access and string literals",
                manifest_path.display()
            );
            return 1;
        }
    };
    let Some(manifest) = read_manifest(&bytes) else {
        eprintln!(
            "rts: '{}' is not a well-formed program-data file",
            manifest_path.display()
        );
        return 1;
    };

    // The region: allocated BEFORE its address is published anywhere, so there
    // is no window in which the cell reads a stale value. See this module's own
    // doc comment for why the write must precede everything below it.
    let region = Region::with_capacity(CELLS);
    // SAFETY: the only write to this cell in the process, made before the
    // first call into compiled code and before any native that could allocate.
    unsafe {
        REGION_BASE = region.base();
    }

    let singletons = Singletons {
        undefined: manifest.singletons[0],
        null: manifest.singletons[1],
        hole: manifest.singletons[2],
    };
    let kinds = rts_core::Kinds {
        symbol: manifest.kinds[0],
        bigint: manifest.kinds[1],
    };

    let mut context = Context::over(singletons, kinds, region);
    context.stack_high = current_thread_stack_high();
    rts_core::entry::declare_keys(&mut context, &manifest.keys);
    rts_core::entry::declare_literals(&mut context, &manifest.literals);
    rts_core::entry::declare_templates(&mut context, &manifest.templates);
    // No generator bodies and no stack-trace names yet — see
    // `rts_host::object`'s stated gap. An empty table each: a generator
    // placed by this path resumes into a zeroed shape, and an uncaught throw's
    // message carries no `at name` line, both named there rather than guessed
    // at here.
    rts_core::entry::declare_frames(&mut context, Vec::new());
    rts_core::entry::declare_function_names(&mut context, Vec::new());
    // `vm.runInNewContext` needs a compiler this binary does not carry — an AOT
    // program that reaches for one gets the honest absence rather than a link
    // against `rts-codegen`, which would pull the whole front end into every
    // compiled binary for a feature most programs never touch.
    rts_core::entry::declare_rest(&mut context, |wait: Duration| std::thread::sleep(wait));

    rts_std::install(&mut context);
    rts_node::install(&mut context);

    let nothing = singletons.undefined as u64;
    let (_, exit_code) = rts_core::entry::with_context(context, || {
        // SAFETY: `__rts_script` is exported by the object under the ABI every
        // compiled function uses, which this call matches by construction —
        // `Entry`'s definition and `object::compile_to_object`'s placement are
        // the same convention `rts_codegen::emit::emit_program` builds. The
        // `unsafe extern "C"` block above is itself the declaration that this
        // symbol exists and takes this shape; nothing further can be checked
        // from this side, same as any other FFI boundary.
        let _ = unsafe { __rts_script(nothing, nothing, nothing, nothing, nothing, nothing) };

        // The event loop: drain what is queued, ask every registered source to
        // deliver, wait, repeat — the same loop `run_region` runs, for the same
        // reason (an earlier AOT shipped without it and starved every timer).
        loop {
            rts_core::entry::drain_microtasks();
            let Some(wait) = rts_core::entry::pump_sources() else {
                break;
            };
            std::thread::sleep(wait);
        }
        rts_core::entry::drain_microtasks();

        if let Some((tag, described)) = rts_core::entry::pending() {
            eprintln!("rts: uncaught exception (tag {tag}): {described}");
            1
        } else {
            0
        }
    });
    exit_code
}
