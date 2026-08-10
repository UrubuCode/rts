//! Which files a program is made of, and in what order they run.
//!
//! # Why the host owns this and not the language
//!
//! Reading a file and turning `"./other.ts"` into a path are the host's, exactly
//! as `rts-core`'s `modules` doc says. What an import MEANS for a scope is
//! the language's and stays in `rts-codegen`. So this file resolves and reads,
//! and hands the language a list of parsed modules in the order they must run.
//!
//! # Why every module of a program is ONE compilation
//!
//! Because a reference belongs to the region that made it. A module compiled and
//! run on its own would hold its exports in its own region and the importer, in
//! another, could not touch them — the same wall `node:vm` and `worker_threads`
//! hit. This does not cross that wall: it collects the whole graph first, and
//! `rts-codegen`'s `emit_modules` emits all of it into one compilation with one
//! literal table, one key registry and one region.
//!
//! # What a specifier means here
//!
//! A relative one (`./x`, `../x`) resolves against the directory of the file
//! that wrote it. Anything else — `rts:test`, `node:fs`, a bare name — is left
//! alone: those are answered from the table the host filled before the program
//! runs, and a loader that tried to find `node:fs` on disk would shadow the real
//! one with whatever happened to be there.
//!
//! An extension is not GUESSED, and this rule changed once with its reason. It
//! used to be absolute — `./x` is `./x` — because a resolver that tried `.ts`,
//! then `.js`, then `/index.ts` picks a file the program did not name, and
//! which one it picked is invisible until two of them exist.
//!
//! What is here now is narrower and keeps that property: `./x` is tried, and if
//! nothing is there, `./x.ts` — one candidate, not a cascade. Two files can
//! never both match, so there is nothing invisible to pick between. It exists
//! because that is how TypeScript is written: every relative import in this
//! repository's own suite omits the extension, and refusing them measured as an
//! engine that cannot compile modules rather than as a resolver that will not
//! look.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rts_codegen::names::Names;
use rts_codegen::parse::parse_module;
use rts_codegen::syntax::ModuleItem;

use crate::link::HostError;

/// One file of the graph, in the order it must run.
///
/// The SOURCE, not a parsed tree. The walk below parses each file to find its
/// imports and throws that tree away, because a `Name` is an index into a table
/// and the walk's table is not the compilation's — a tree carried out of here
/// would name locals by numbers nothing else has issued.
pub struct Loaded {
    /// What an import of it names — the resolved path, as text.
    pub specifier: String,
    /// Where it came from, for resolving ITS imports.
    pub path: PathBuf,
    /// Its text.
    pub source: String,
}

/// Whether a specifier names a file rather than something the host provides.
fn is_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

/// The files one module names, in source order, ignoring everything the host
/// provides by name (`node:`, `rts`, a bare specifier).
///
/// Public because a second caller reads a module before it is on disk: `rts run
/// https://…` mirrors a program and its imports into a temp directory, and it
/// has to know what to fetch next. That caller used to parse with the OLD
/// engine's parser and walk the OLD engine's AST, which answered a slightly
/// different question — a specifier this engine would refuse could be mirrored
/// happily, and vice versa. One answer, from the parser that will compile it.
///
/// The tree is thrown away: this wants the specifiers and nothing else, and a
/// `Name` is an index into a table this parse owns and no other compilation
/// issued.
pub fn relative_imports(source: &str) -> Result<Vec<String>, String> {
    let mut scratch = Names::default();
    let parsed = parse_module(source, &mut scratch).map_err(|error| format!("{error:?}"))?;
    let mut found = Vec::new();
    for item in &parsed.body {
        let specifier = match item {
            ModuleItem::Import(import) => import.source.clone(),
            ModuleItem::Export(export) => match &export.kind {
                rts_codegen::syntax::ExportKind::Named {
                    source: Some(from), ..
                } => from.clone(),
                rts_codegen::syntax::ExportKind::All { source, .. } => source.clone(),
                _ => continue,
            },
            ModuleItem::Stmt(_) => continue,
        };
        if is_relative(&specifier) {
            found.push(specifier);
        }
    }
    Ok(found)
}

/// The path a relative specifier names, from the file that wrote it.
fn resolve(from: &Path, specifier: &str) -> PathBuf {
    let base = from.parent().unwrap_or(Path::new("."));
    let named = base.join(specifier);
    // What the program wrote, and then the one candidate: `./x` before `./x.ts`.
    // Written this way round so a file that genuinely has no extension still
    // wins over a `.ts` beside it — the program named that one.
    let joined = match named.exists() {
        true => named,
        false => {
            let with_ts = base.join(format!("{specifier}.ts"));
            match with_ts.exists() {
                true => with_ts,
                false => named,
            }
        }
    };
    // Canonicalised so that `./a.ts` and `../dir/a.ts` are the SAME module. Two
    // spellings of one file compiled twice would run its side effects twice and
    // give it two namespaces, and `import { x } from` each would answer two
    // different `x`.
    joined.canonicalize().unwrap_or(joined)
}

/// Reads the whole graph reachable from `entry`, dependencies first.
///
/// # Order
///
/// Post-order depth first: a module is emitted after everything it imports, so
/// by the time its body runs, every namespace it reads has been published.
///
/// # Cycles
///
/// Refused by name. A cycle needs the importing module to see a binding that
/// does not have a value yet — which is what a live binding and its temporal
/// dead zone are for, and neither exists here. Detecting it and saying so beats
/// running one of the two modules against a namespace that is still empty,
/// which would answer `undefined` for a name that is genuinely there.
pub fn load(entry: &Path) -> Result<Vec<Loaded>, HostError> {
    let start = entry.canonicalize().unwrap_or_else(|_| entry.to_owned());
    let mut ordered = Vec::new();
    let mut state = HashMap::new();
    visit(&start, &mut ordered, &mut state)?;
    Ok(ordered)
}

/// Where one file is in the walk.
#[derive(Clone, Copy, PartialEq)]
enum Mark {
    /// On the stack — reaching it again is a cycle.
    Open,
    /// Finished and already in the order.
    Done,
}

fn visit(
    path: &Path,
    ordered: &mut Vec<Loaded>,
    state: &mut HashMap<PathBuf, Mark>,
) -> Result<(), HostError> {
    match state.get(path) {
        Some(Mark::Done) => return Ok(()),
        Some(Mark::Open) => {
            return Err(HostError::Parse(format!(
                "{} is part of an import cycle, which this engine does not link",
                path.display()
            )));
        }
        None => {}
    }
    state.insert(path.to_owned(), Mark::Open);

    let source = std::fs::read_to_string(path)
        .map_err(|error| HostError::Parse(format!("{}: {error}", path.display())))?;
    // Parsed with a `Names` of its own, and thrown away: this pass wants the
    // import specifiers and nothing else. The real parse happens against the
    // `Names` the whole compilation shares.
    for specifier in relative_imports(&source)
        .map_err(|error| HostError::Parse(format!("{}: {error}", path.display())))?
    {
        visit(&resolve(path, &specifier), ordered, state)?;
    }

    state.insert(path.to_owned(), Mark::Done);
    ordered.push(Loaded {
        specifier: path.display().to_string(),
        path: path.to_owned(),
        source,
    });
    Ok(())
}

/// What a module's imports must be rewritten to, so a relative specifier names
/// the same thing the loader resolved it to.
///
/// # Why the tree is rewritten rather than the runtime taught to resolve
///
/// The runtime's table is keyed by the exact text an import wrote, and the same
/// text means different files in two directories. Resolving once, here, and
/// writing the resolved name into both sides — the import that reads and the
/// export that publishes — keeps the runtime's lookup a plain string comparison
/// and keeps path resolution in the one place that read the files.
pub fn rewrite(items: &mut [ModuleItem], from: &Path) {
    for item in items {
        match item {
            ModuleItem::Import(import) => {
                if is_relative(&import.source) {
                    import.source = resolve(from, &import.source).display().to_string();
                }
            }
            ModuleItem::Export(export) => match &mut export.kind {
                rts_codegen::syntax::ExportKind::Named {
                    source: Some(source),
                    ..
                } => {
                    if is_relative(source) {
                        *source = resolve(from, source).display().to_string();
                    }
                }
                rts_codegen::syntax::ExportKind::All { source, .. } => {
                    if is_relative(source) {
                        *source = resolve(from, source).display().to_string();
                    }
                }
                _ => {}
            },
            ModuleItem::Stmt(_) => {}
        }
    }
}

/// Everything `compile_graph` does before placement: the graph loaded, every
/// file parsed against ONE name table, and all of them emitted into one program.
///
/// # Why it lives beside the loader rather than beside the compile
///
/// Two callers need it — `run::compile_graph`, which places it, and
/// [`crate::describe`], which prints it — so it could not stay inside either.
/// It landed here because everything it does is about the GRAPH: the load, the
/// specifier rewriting, and the one name table all the files are parsed
/// against. `run.rs` is also over this crate's 500-line ceiling (rule 6) and
/// adding to it was the wrong direction.
///
/// The returned list is the module initialisers to run before the entry, with
/// the entry itself removed — it is what `assemble` is handed as `before`.
pub(crate) fn front_end(
    entry: &Path,
) -> Result<(crate::run::FrontEnd, Vec<rts_cranelift::ir::FuncId>), HostError> {
    let loaded = load(entry)?;
    let mut names = Names::default();

    // Parsed HERE, against the `Names` the whole compilation shares — the walk
    // that found the graph parsed each file too, with a table of its own, and
    // threw the trees away. A `Name` is an index, so a tree from that walk would
    // name locals by numbers this compilation never issued.
    let mut parsed = Vec::with_capacity(loaded.len());
    for file in &loaded {
        let mut program = parse_module(&file.source, &mut names)
            .map_err(|error| HostError::Parse(format!("{}: {error:?}", file.specifier)))?;
        // Every relative specifier becomes the path the loader resolved it to,
        // on BOTH sides: the import that reads and the re-export that forwards.
        // The runtime's table is a string comparison, and `./x` means different
        // files in two directories.
        rewrite(&mut program.body, &file.path);
        parsed.push(program);
    }

    let mut tags = rts_cranelift::tags::TagRegistry::new();
    let model = rts_codegen::values::ValueModel::declare(&mut tags);
    let types = rts_cranelift::types::TypeRegistry::new();
    let mut funcs = rts_cranelift::ir::FuncRegistry::new();
    let mut calls = rts_codegen::runtime::RuntimeCalls::new();
    let mut keys = rts_cranelift::shape::KeyRegistry::new();

    let units: Vec<rts_codegen::emit::Unit<'_>> = loaded
        .iter()
        .zip(&parsed)
        .map(|(file, program)| rts_codegen::emit::Unit {
            specifier: file.specifier.clone(),
            items: &program.body,
        })
        .collect();

    let emitted = {
        let mut ctx = rts_codegen::emit::Ctx::new(
            &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
        );
        match rts_codegen::emit::emit_modules(&units, &mut ctx) {
            Err(rts_codegen::emit::EmitError::UnboundName(name)) => {
                return Err(HostError::Unbound(ctx.names.text(name).to_owned()));
            }
            other => other?,
        }
    };
    // The last unit is the entry — the file the caller named — and everything
    // before it is a dependency the loader ordered.
    let mut entries = emitted.entries;

    entries.pop();
    Ok((
        crate::run::FrontEnd {
            emitted: emitted.program,
            model,
            funcs,
            types,
            calls,
            names,
        },
        entries,
    ))
}
