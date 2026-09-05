//! Placing a program into an object file instead of this process's memory.
//!
//! # What this shares with [`crate::run`], and what it does not
//!
//! Rule 4 of this crate's `README.md`: "both destinations, or neither... the
//! difference is stated and is about the destination — not about what was
//! compiled." Parsing, emission, the generator rewrite, the runtime-operation
//! agreement check and the verifier are [`crate::run::front_end`] and
//! [`crate::run::prepare`] — shared, unchanged, called from here exactly as
//! [`crate::run::compile_for`] calls them. A GRAPH of modules is
//! [`crate::graph::front_end`], shared with [`crate::run::compile_graph`] the
//! same way. Everything in *this* file is about where the bytes end up and what
//! a linker needs to finish the job.
//!
//! # The region base
//!
//! An in-memory run bakes the heap's address into the code as an immediate,
//! because the host that allocates the region already knows where it put it.
//! An object file cannot do that — the region does not exist until the binary
//! runs — so the heap here is [`RegionBase::Symbol`]: a named cell the object
//! leaves undefined, for the archive that defines `main` to fill in before the
//! first allocation. See `rts-runtime/src/aot/mod.rs` for the write, and for
//! why its ordering is the one thing about this design that cannot be gotten
//! wrong quietly.
//!
//! # What crosses beside the object bytes, and why it is not IN them
//!
//! Property keys, string literals and template pieces are per-COMPILATION data
//! that the language names by NUMBER — `declare_keys`/`declare_literals`/
//! `declare_templates` seed the very tables those numbers index into, and this
//! is the same seeding [`crate::run::run_region`] does for a JIT run, just
//! before the entry is ever called. An AOT binary needs the identical seed at
//! the identical moment.
//!
//! So the numbers travel as a small sidecar file next to the executable,
//! written by [`write_manifest`] and read by the facade's own `main` before it
//! calls the compiled entry. This is the one place this design is not fully
//! self-contained in a single file, and it is named here rather than left to be
//! discovered: moving an AOT binary without its `.rtsdata` sidecar breaks it.
//!
//! # And what could NOT travel that way, until the machine grew a table
//!
//! Three things the runtime is seeded with are keyed by a CODE ADDRESS: which
//! module bodies run before the entry, what a parked generator frame looks
//! like, and what each compiled function is called. An address does not exist
//! until a linker places the object, in a process this crate never runs, so
//! none of them could be written into a manifest and all three shipped EMPTY.
//!
//! That was not free, and it is worth recording what it cost, because the
//! sentence that used to stand here called it "stated rather than hidden" and
//! two thirds of it were silent:
//!
//! - a program with a relative import was REFUSED, so there was no AOT path for
//!   any program of more than one file;
//! - `async` and generators raised `async function has no registered frame` —
//!   loud, at least;
//! - and `f.name`, `f.length`, `f.toString()`, whether an arrow wrongly got a
//!   `prototype`, and every `at …` line of every stack trace were **wrong with
//!   no error at all**, because `declare_function_names` seeds all of that and
//!   an empty table answers `undefined` for each.
//!
//! [`rts_cranelift::target::AddressTable`] is what closes it: a data symbol of
//! one relocation per function, filled in by the LINKER as it places the code.
//! The manifest carries what the compiler knew and the tables carry what only
//! the linker knows, and neither restates the other.

pub mod html_scripts;
mod manifest;
mod page;

use std::collections::HashSet;

use rts_cranelift::ir::FuncId;
use rts_cranelift::mem::{RegionBase, RegionBases};
use rts_cranelift::target::{
    AddressTable, Placing, Visibility, object_file, place_in_object,
};

use crate::link::HostError;
use crate::run::{FrontEnd, Prepared, prepare};

/// The name of the cell compiled code reads the region's base address from.
///
/// Declared into the object as [`RegionBase::Symbol`], left undefined there, and
/// defined by `rts-runtime`'s `main` — see this module's own doc comment.
pub const REGION_BASE_SYMBOL: &str = "__rts_region_base";

/// The module bodies that run before the entry, in the order they must run.
pub const MODULE_TABLE_SYMBOL: &str = "__rts_modules";

/// The rewritten generator bodies, in the order this program's frame shapes are
/// written to the manifest.
pub const FRAME_TABLE_SYMBOL: &str = "__rts_frames";

/// Every placed function, in the order this program's function names are
/// written to the manifest.
pub const FUNCTION_TABLE_SYMBOL: &str = "__rts_functions";

/// A compiled program, placed into an object file's bytes, plus what the
/// facade's `main` has to seed before calling `__rts_script`.
pub struct ObjectProgram {
    /// The relocatable object. Undefined names: every `RuntimeOp` this program
    /// calls, every `RtEntry` the machine emits, and [`REGION_BASE_SYMBOL`] —
    /// all left for a linker to resolve against the `rts-runtime` archive.
    pub bytes: Vec<u8>,
    /// What the compiler numbered the singletons — copied out because a linked
    /// binary has no [`ValueModel`] to ask; see [`crate::link::singletons_for`].
    pub singletons: [u32; 3],
    /// The kinds the language declared for itself — see
    /// [`crate::link::kinds_for`].
    pub kinds: [u8; 2],
    /// Every property key text the compilation minted, in key order. Seeded by
    /// `declare_keys` before the entry runs — same requirement `run_region`
    /// states, same order.
    pub keys: Vec<String>,
    /// Every string literal, by position, as UTF-16 code units. Seeded by
    /// `declare_literals`, which states why it is units and not `String`.
    pub literals: Vec<Vec<u16>>,
    /// Every tagged-template site, by literal position. Seeded by
    /// `declare_templates`, after the literals.
    pub templates: Vec<Vec<u32>>,
    /// How many module bodies run before the entry.
    ///
    /// A COUNT and not a list, because the list is the linker's: entry `n` of
    /// [`MODULE_TABLE_SYMBOL`] is the `n`-th of them, and this says how far to
    /// read. Zero for a program of one file.
    pub modules: u32,
    /// What a parked frame looks like, per generator body — minus the address,
    /// which is entry `n` of [`FRAME_TABLE_SYMBOL`].
    ///
    /// `code` is `0` in every one of these and is not what the runtime uses:
    /// the facade overwrites it from the table before seeding. It is carried in
    /// the shape rather than beside it because `FrameShape` is the runtime's
    /// own type, and inventing a second one to hold the same fields minus one
    /// is two spellings of one record.
    pub frames: Vec<rts_core::entry::FrameShape>,
    /// What each placed function is called, its declared arity, whether it has
    /// a `prototype` and whether it constructs — minus the address, which is
    /// entry `n` of [`FUNCTION_TABLE_SYMBOL`].
    pub function_names: Vec<(String, u32, bool, bool)>,
    /// What `import.meta` answers, per module of the graph.
    ///
    /// No address in it at all — a specifier, a URL and a boolean — so it could
    /// have ridden the manifest from the beginning. It did not, because there
    /// was no graph to describe.
    pub module_metas: Vec<crate::graph::ModuleMeta>,
    /// Every `(referrer, written, resolved)` the loader resolved, for the
    /// resolver an AOT binary installs.
    ///
    /// A static specifier is rewritten into the tree and needs none of this. A
    /// `require("./x")` or an `import("./x")` asks at RUN time, and a JIT run
    /// answers by looking at the disk the loader just read — which an AOT
    /// binary, running anywhere, cannot. So the answers travel.
    ///
    /// Carried rather than re-derived, because a second resolver is a second
    /// thing that has to agree with the first. See [`crate::graph::Loaded`]'s
    /// own field for what disagreeing cost the last time.
    pub resolutions: Vec<(String, String, String)>,
    /// One entry per page `<script>` this program was compiled with
    /// (`--html`): the hash of its exact source, paired with its position in
    /// [`Self::function_names`] — which is [`FUNCTION_TABLE_SYMBOL`]'s own
    /// order, so the address is the linker's the same way every other entry's
    /// is. Empty for a program compiled without `--html`; see [`page`] for
    /// how they are placed, and `rts-runtime`'s `aot::page_scripts` for how
    /// this table is read back and turned into
    /// `context.eval_compiler_with_receiver`.
    pub page_scripts: Vec<(u64, u32)>,
}

/// Compiles source text into an object file, ready to link against the
/// `rts-runtime` archive.
///
/// One file, with no imports. A program that names another file is
/// [`compile_graph_to_object`], for the reason `crate::graph` gives: every
/// module of a program is ONE compilation, because a reference belongs to the
/// region that made it.
///
/// The two empty lists below are the same fact twice: source text has no file,
/// so it has no `import.meta` to answer and no directory to resolve `"./x"`
/// against.
pub fn compile_to_object(source: &str) -> Result<ObjectProgram, HostError> {
    place(crate::run::front_end(source)?, &[], Vec::new(), Vec::new(), &[])
}

/// The same, plus every page `<script>` `extract_files` found in the caller's
/// `--html` files — extracted, compiled and placed into this SAME object, so
/// `rts-runtime`'s `eval_compiler_with_receiver` hook can find one by the
/// hash of its source. See [`page`]'s own header for why they cannot be a
/// second object file, and [`html_scripts`] for the extraction rule.
///
/// `page_scripts` is the extracted SOURCE TEXT, in the order the manifest
/// records them — `crate::object::html_scripts::extract_files` is how a
/// caller builds it from a list of `--html` paths. An empty slice is
/// [`compile_to_object`] exactly: this function costs nothing extra when
/// there is nothing to precompile.
pub fn compile_to_object_with_html(
    source: &str,
    page_scripts: &[String],
) -> Result<ObjectProgram, HostError> {
    place(
        crate::run::front_end(source)?,
        &[],
        Vec::new(),
        Vec::new(),
        page_scripts,
    )
}

/// Compiles a file and everything it imports into ONE object file.
///
/// # Why this is not "compile each module and let the linker join them"
///
/// Because that is not what a module is here. `crate::graph`'s own header has
/// it: every module of a program shares one compilation, one literal table, one
/// key registry and one region, since a reference belongs to the region that
/// made it. Separate objects would be separate compilations — separate key
/// numberings, separate literal tables — and a linker cannot reconcile two
/// numberings of one space.
///
/// So the graph is loaded and emitted exactly as [`crate::run::compile_graph`]
/// loads and emits it, and what differs is only the destination: the module
/// bodies' addresses go into [`MODULE_TABLE_SYMBOL`] for a linker to fill in,
/// where an in-memory run reads them straight out of the placement.
pub fn compile_graph_to_object(entry: &std::path::Path) -> Result<ObjectProgram, HostError> {
    let graph = crate::graph::front_end(entry)?;
    place(graph.front, &graph.before, graph.metas, graph.resolutions, &[])
}

/// The same, plus `--html` page scripts — see [`compile_to_object_with_html`],
/// which this mirrors for a program of more than one file.
pub fn compile_graph_to_object_with_html(
    entry: &std::path::Path,
    page_scripts: &[String],
) -> Result<ObjectProgram, HostError> {
    let graph = crate::graph::front_end(entry)?;
    place(
        graph.front,
        &graph.before,
        graph.metas,
        graph.resolutions,
        page_scripts,
    )
}

/// Everything both entry points share: prepare, name, place, and collect what
/// has to cross beside the bytes.
///
/// `page_scripts` is placed into `front` BEFORE `prepare` ever sees it —
/// [`page::extend`] appends their functions, names, literals and templates
/// into `front.emitted` first, so every table `prepare`/`place_in_object`
/// build from `front` already counts them. Nothing downstream of that line
/// needs to know a page script is a different kind of function from any
/// other this compilation placed.
fn place(
    front: FrontEnd,
    before: &[FuncId],
    module_metas: Vec<crate::graph::ModuleMeta>,
    resolutions: Vec<(String, String, String)>,
    page_scripts: &[String],
) -> Result<ObjectProgram, HostError> {
    let (front, page_hashes) = page::extend(front, page_scripts)?;
    let model = front.model;
    let names = front.names;
    let Prepared {
        emitted,
        funcs,
        types,
        script: _,
        frames,
        expected,
        names_for_placing,
    } = prepare(front.emitted, front.funcs, front.types, front.calls)?;

    let mut placing: Vec<Placing<'_>> = expected
        .iter()
        .map(|(op, id)| Placing {
            id: *id,
            name: op.symbol(),
            visibility: Visibility::Expected,
            body: None,
        })
        .collect();
    for ((id, body), name) in emitted.functions.iter().zip(&names_for_placing) {
        placing.push(Placing {
            id: *id,
            name,
            visibility: Visibility::Exported,
            body: Some(body),
        });
    }

    // Only what this object DEFINES may be in a table: a relocation names a
    // function symbol, and one that was declared and never given a body has no
    // address for a linker to write. This is the same filter
    // `crate::run::addressed` applies through `address_of` returning `None`,
    // asked of the other destination.
    let placed: HashSet<FuncId> = emitted.functions.iter().map(|(id, _)| *id).collect();
    let module_ids: Vec<FuncId> = before.iter().copied().filter(|id| placed.contains(id)).collect();
    let frame_ids: Vec<FuncId> = frames
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| placed.contains(id))
        .collect();
    let frame_shapes: Vec<rts_core::entry::FrameShape> = frames
        .iter()
        .filter(|(id, _)| placed.contains(id))
        .map(|(_, shape)| shape.clone())
        .collect();
    let named: Vec<&(FuncId, String, u32, bool, bool)> = emitted
        .function_names
        .iter()
        .filter(|(id, ..)| placed.contains(id))
        .collect();
    let function_ids: Vec<FuncId> = named.iter().map(|(id, ..)| *id).collect();
    let function_names: Vec<(String, u32, bool, bool)> = named
        .iter()
        .map(|(_, name, arity, has_prototype, constructs)| {
            (name.clone(), *arity, *has_prototype, *constructs)
        })
        .collect();

    // Each page script's `FuncId`, turned into its position in `function_ids`
    // — the same order `FUNCTION_TABLE_SYMBOL` places addresses in, so this
    // index is what `rts-runtime`'s `main` can hand straight to `addresses`'s
    // result with no further translation. Looked up here rather than carried
    // from `page::extend`, because that is BEFORE `prepare`'s own rewrite
    // (generators) and BEFORE this function's OWN `placed` filter — either
    // could in principle drop or renumber an id, and the position that
    // matters is the one AFTER both have run.
    let page_scripts: Vec<(u64, u32)> = page_hashes
        .iter()
        .filter_map(|(hash, id)| {
            function_ids
                .iter()
                .position(|placed_id| placed_id == id)
                .map(|position| (*hash, position as u32))
        })
        .collect();

    let tables = [
        AddressTable {
            name: MODULE_TABLE_SYMBOL,
            functions: &module_ids,
        },
        AddressTable {
            name: FRAME_TABLE_SYMBOL,
            functions: &frame_ids,
        },
        AddressTable {
            name: FUNCTION_TABLE_SYMBOL,
            functions: &function_ids,
        },
    ];

    // Single region, symbolic base: the address is not known until this binary
    // runs. Stride is a machine constant, not a fact about any particular
    // `Region` — see `rts_core::heap::STRIDE`'s own doc comment.
    let bases = RegionBases::single(
        RegionBase::Symbol(REGION_BASE_SYMBOL.to_owned()),
        rts_core::heap::STRIDE,
    );

    let object = object_file("rts_program")?;
    let bytes = place_in_object(object, &placing, &tables, &funcs, &types, Some(bases))?;

    let singletons = crate::link::singletons_for(&model);
    let kinds = crate::link::kinds_for(&model);

    Ok(ObjectProgram {
        bytes,
        singletons: [singletons.undefined, singletons.null, singletons.hole],
        kinds: [kinds.symbol, kinds.bigint],
        keys: names
            .keyed_texts()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        literals: emitted.literals,
        templates: emitted.templates,
        modules: module_ids.len() as u32,
        frames: frame_shapes,
        function_names,
        module_metas,
        resolutions,
        page_scripts,
    })
}

/// Writes everything but the object bytes to `path`, in the format
/// `rts-runtime`'s `main` reads. See [`manifest`] for that format.
pub fn write_manifest(path: &std::path::Path, program: &ObjectProgram) -> std::io::Result<()> {
    manifest::write(path, program)
}
