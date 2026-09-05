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
//! So the object built by `rts_host::object` exports its script under the fixed
//! name `__rts_script`, under the exact ABI convention every compiled function
//! uses, and this module supplies the process's actual `main`: it runs the same
//! sequence `run_region` runs, then calls that symbol. One startup, two callers
//! — this file's job is to be the second, not to invent a third.
//!
//! # What a MODULE GRAPH added, and why it is three symbols and not one
//!
//! A program of several files has one compiled body per file, and they run
//! dependencies-first before the entry. `run_region` reads their addresses
//! straight out of the placement; there is no placement here, so the addresses
//! arrive as a table the LINKER filled in — `__rts_modules`, and beside it
//! `__rts_frames` and `__rts_functions` for the two other tables that are keyed
//! by a code address. `rts_host::object`'s header has the design and what all
//! three shipping empty used to cost.
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

mod manifest;
mod page_scripts;
mod resolver;

use std::time::Duration;

use rts_core::entry::Context;
use rts_core::heap::Region;
use rts_core::value::Singletons;

/// How the compiled program is entered — its script, and each module body that
/// runs before it.
///
/// One convention for both, because there is one: a module body is an ordinary
/// compiled function, and `rts_host::run`'s own `Entry` spells the same six
/// words. A second shape for the entry would be a second thing to keep in
/// agreement with the callee.
type Entry = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

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

    /// The module bodies that run before the entry, in order.
    ///
    /// One pointer-sized word per module, written by the LINKER — see
    /// `rts_cranelift::target::AddressTable` for why this is the only way an
    /// object file can say where a function ended up, and
    /// `rts_host::object::MODULE_TABLE_SYMBOL` for the name.
    ///
    /// Typed `u8` and read through a raw pointer rather than declared as an
    /// array: how many entries there are is the manifest's to say, and an
    /// array type would have to state a length this declaration cannot know.
    #[link_name = "__rts_modules"]
    static MODULE_TABLE: u8;

    /// The rewritten generator bodies, in the manifest's frame order.
    #[link_name = "__rts_frames"]
    static FRAME_TABLE: u8;

    /// Every placed function, in the manifest's function order.
    #[link_name = "__rts_functions"]
    static FUNCTION_TABLE: u8;
}

/// The addresses in one of the tables above, or `None` if it does not hold as
/// many as `expected`.
///
/// # Why the table is asked instead of believed
///
/// `expected` comes from the manifest, which is a SEPARATE FILE. Two files are
/// two things that can disagree: a link that succeeded followed by a manifest
/// write that failed leaves a fresh executable beside an older, smaller
/// sidecar, and this module's own header already warns that an executable moved
/// without its sidecar is broken. Believing the sidecar's count would then read
/// past the end of the table the linker sized — and for the module table, the
/// word past the end is transmuted into a function pointer and CALLED.
///
/// So the table states its own length in its first word
/// (`rts_cranelift::target::AddressTable`), and a mismatch is refused here
/// rather than discovered as a jump into whatever the linker placed next.
///
/// # Safety
///
/// `table` must be one of the three symbols declared above. Nothing else about
/// the caller is trusted: the length is read out of the table itself.
unsafe fn addresses(table: *const u8, expected: usize) -> Option<Vec<u64>> {
    let base = table as *const u64;
    // SAFETY: the table is aligned to the pointer width by
    // `define_address_table` and is at least one word long — the count — even
    // when it holds no entries.
    let count = unsafe { base.read() } as usize;
    if count != expected {
        return None;
    }
    // SAFETY: `count` words follow the one just read, by the same writer that
    // wrote the count.
    Some((0..count).map(|at| unsafe { base.add(at + 1).read() }).collect())
}

/// The top of the CURRENT thread's stack on a platform with a verified method.
///
/// Duplicated from `rts-host::stack` rather than shared: that crate is not
/// a dependency of this one (naming it would be backwards — the facade is what
/// an AOT binary links against, and `rts-host` is a JIT host that itself
/// depends on nothing this crate produces). Both destinations must seed the
/// same collector contract, but the small platform adapters remain local to
/// the binaries that own their startup paths.
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

/// The top of the current Linux thread's stack, from its pthread attributes.
#[cfg(target_os = "linux")]
fn current_thread_stack_high() -> Option<usize> {
    let mut attributes = std::mem::MaybeUninit::<libc::pthread_attr_t>::uninit();
    // SAFETY: `pthread_getattr_np` initializes `attributes` when it returns 0;
    // `pthread_self` names this thread, so the returned bounds belong to the
    // stack that will later be scanned by the collector.
    let status = unsafe { libc::pthread_getattr_np(libc::pthread_self(), attributes.as_mut_ptr()) };
    if status != 0 {
        return None;
    }

    let mut attributes = unsafe { attributes.assume_init() };
    let mut base = std::ptr::null_mut();
    let mut size = 0usize;
    // SAFETY: `attributes` was initialized by pthread_getattr_np and both
    // output pointers are valid for the duration of this call.
    let status = unsafe { libc::pthread_attr_getstack(&attributes, &mut base, &mut size) };
    // SAFETY: pthread_attr_destroy accepts an initialized pthread attribute.
    let destroy_status = unsafe { libc::pthread_attr_destroy(&mut attributes) };
    if status != 0 || destroy_status != 0 || base.is_null() {
        return None;
    }
    Some(base as usize + size)
}

/// The honest answer on platforms without a verified stack-top mechanism.
#[cfg(not(any(target_os = "linux", all(target_arch = "x86_64", target_os = "windows"))))]
fn current_thread_stack_high() -> Option<usize> {
    None
}

/// Cells the AOT binary allocates, one per program — one region, same as
/// `rts_host::run::compile`'s default.
const CELLS: u32 = 1 << 16;

/// The process's real entry point, for a binary the object file's
/// `__rts_script` was linked into.
///
/// `#[cfg(not(test))]` because `cargo test`'s own harness needs a `main` of
/// its own, and a `staticlib`'s exported one collided with it at LINK time —
/// `error: main already defined`, plus the four undefined symbols this
/// function names, none of which the test binary's object graph provides.
/// That is a fact about the test harness rather than about a real AOT link,
/// which never sets `cfg(test)`: this symbol is exported exactly as before in
/// every archive `rts compile` actually links against. Discovered adding this
/// batch's `page_scripts` unit tests, which live in the same module this
/// function does and could not otherwise run at all — `cargo test -p
/// rts-runtime` had never once succeeded.
///
/// # Safety
///
/// Called by the C runtime with the platform's own `argc`/`argv` convention.
/// Nothing here reads them; the manifest is found next to the running
/// executable rather than on the command line.
#[cfg(not(test))]
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
    let Some(manifest) = manifest::read(&bytes) else {
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

    // The three tables the LINKER filled in. Each is the same list the
    // manifest describes, in the same order, with the one field the compiler
    // could not know: an address.
    //
    // Each is CHECKED against the manifest's count rather than sized by it —
    // see [`addresses`] for the stale-sidecar case that makes the difference a
    // jump into arbitrary code.
    //
    // SAFETY: the three symbols are the ones declared above.
    let tables = unsafe {
        (
            addresses(&raw const FRAME_TABLE, manifest.frames.len()),
            addresses(&raw const FUNCTION_TABLE, manifest.functions.len()),
            addresses(&raw const MODULE_TABLE, manifest.modules),
        )
    };
    let (Some(frames), Some(functions), Some(modules)) = tables else {
        eprintln!(
            "rts: '{}' does not describe this executable — it was written by a \
             different compilation. An AOT binary and its .rtsdata sidecar are one \
             thing; re-run `rts compile`.",
            manifest_path.display()
        );
        return 1;
    };

    // What a parked frame looks like, per generator body. Without this an
    // `async` function or a generator answered "has no registered frame" — the
    // loud third of what an empty seed cost.
    let shapes: Vec<rts_core::entry::FrameShape> = manifest
        .frames
        .into_iter()
        .zip(&frames)
        .map(|(shape, code)| rts_core::entry::FrameShape {
            code: *code,
            ..shape
        })
        .collect();
    rts_core::entry::declare_frames(&mut context, shapes);
    // And what each function is called. This is the SILENT third: an empty
    // table left `f.name` and `f.length` undefined, gave every arrow a
    // `prototype`, rendered every `toString()` as `[native code]` and emptied
    // every stack trace — none of it raising anything.
    let named: Vec<(u64, String, u32, bool, bool)> = manifest
        .functions
        .into_iter()
        .zip(&functions)
        .map(|((name, arity, has_prototype, constructs), at)| {
            (*at, name, arity, has_prototype, constructs)
        })
        .collect();
    rts_core::entry::declare_function_names(&mut context, named);
    // Page `<script>`s `rts compile --html` precompiled, found by the hash of
    // their exact source at run time — see `page_scripts`'s own header for
    // the seam this fills and why a program with no `--html` writes this
    // table empty rather than omitting it. The ADDRESS is `functions[index]`:
    // the manifest names a position in the SAME table `FUNCTION_TABLE_SYMBOL`
    // already resolved, so no second address table exists for this.
    let page_script_entries: Vec<(u64, Entry)> = manifest
        .page_scripts
        .into_iter()
        .filter_map(|(hash, index)| {
            functions
                .get(index as usize)
                // SAFETY: every address in `functions` is a function this
                // object placed under the one convention `Entry` spells,
                // which is what makes reusing the table for a page script's
                // entry safe rather than a second statement of the same fact.
                .map(|address| (hash, unsafe { std::mem::transmute::<u64, Entry>(*address) }))
        })
        .collect();
    page_scripts::declare(page_script_entries);
    rts_core::entry::declare_eval_compiler_with_receiver(
        &mut context,
        page_scripts::evaluate_in_scope_with_receiver,
    );
    // `vm.runInNewContext` needs a compiler this binary does not carry — an AOT
    // program that reaches for one gets the honest absence rather than a link
    // against `rts-codegen`, which would pull the whole front end into every
    // compiled binary for a feature most programs never touch.
    rts_core::entry::declare_rest(&mut context, |wait: Duration| std::thread::sleep(wait));
    // What `require("./x")` and `import("./x")` name. The JIT host installs a
    // resolver that reads the disk; this one reads the answers the loader
    // already found — see `resolver`'s own header for why, and for the one
    // specifier shape it therefore cannot answer.
    resolver::declare(manifest.resolutions);
    rts_core::entry::declare_resolver(&mut context, resolver::resolve);

    // What `import.meta` answers, per module — built HERE, in this program's
    // region, in the same order and out of the same two facts `run_region`
    // builds it from. A module compiled with no meta registered raises rather
    // than answering an empty object, so a graph without this is a program that
    // throws on `import.meta.url`.
    for (specifier, url, main) in &manifest.metas {
        let object = rts_core::entry::make_object(&mut context);
        let url = rts_core::entry::make_string(&mut context, url);
        rts_core::entry::put_member(&mut context, object, "url", url);
        let main = rts_core::entry::boolean_value(*main);
        rts_core::entry::put_member(&mut context, object, "main", main);
        rts_core::entry::declare_module_meta(&mut context, specifier, object);
    }

    rts_std::install(&mut context);
    rts_node::install(&mut context);
    // O mesmo par e a mesma ordem do host JIT (`rts-host/src/run.rs`): o
    // documento é headless e vem sempre; a janela só com a feature `ui`. Sem
    // isto um `.exe` compilado de uma app de UI morria em "cannot resolve
    // module rts:egui" — o comparativo RTS vs Electron foi quem o apanhou.
    rts_dom_bridge::install(&mut context);
    #[cfg(feature = "ui")]
    rts_ui::install(&mut context);

    let nothing = singletons.undefined as u64;
    let (_, exit_code) = rts_core::entry::with_context(context, || {
        // Dependencies first, and their answers dropped — the same order and
        // the same reason `run_region` has: a module publishes its exports as
        // its body finishes, and the importer reads them when its own body
        // starts. The addresses are the linker's; the ORDER is the loader's,
        // and it is the order they were written to the table in.
        for at in &modules {
            // SAFETY: every entry of the module table is a function this same
            // object placed, under the one convention every compiled function
            // uses — which is what `Entry` spells.
            let body: Entry = unsafe { std::mem::transmute::<u64, Entry>(*at) };
            let _ = unsafe { body(nothing, nothing, nothing, nothing, nothing, nothing) };
        }
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
            // A throw the program never caught ENDS it, before the loop asks a
            // source for anything. `run_region` grew this line for a spawned
            // child that kept the process alive after its own script had died,
            // reporting a timeout where the truth was an uncaught exception;
            // this loop is the second copy of that one and had not.
            if rts_core::entry::thrown() != 0 {
                break;
            }
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
    // Depois do programa e antes de qualquer destrutor: um `wgpu::Device` solto
    // num thread-local morre durante o descarregamento das DLLs do driver (ver
    // `rts_ui::shutdown`; o host faz o mesmo). No-op sem janela aberta.
    #[cfg(feature = "ui")]
    rts_ui::shutdown();
    exit_code
}
