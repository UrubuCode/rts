//! Which files a program is made of, and in what order they run.
//!
//! # Why the host owns this and not the language
//!
//! Reading a file and turning `"./other.ts"` into a path are the host's, exactly
//! as `rts-core-rwk`'s `modules` doc says. What an import MEANS for a scope is
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
//! An extension is not guessed. `./x` is `./x`, and `./x.ts` is `./x.ts`; a
//! resolver that tried `.ts`, then `.js`, then `/index.ts` picks a file the
//! program did not name, and which one it picked is invisible until two of them
//! exist.

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

/// The path a relative specifier names, from the file that wrote it.
fn resolve(from: &Path, specifier: &str) -> PathBuf {
    let base = from.parent().unwrap_or(Path::new("."));
    let joined = base.join(specifier);
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
    let mut scratch = Names::default();
    let parsed = parse_module(&source, &mut scratch)
        .map_err(|error| HostError::Parse(format!("{}: {error:?}", path.display())))?;
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
        if !is_relative(&specifier) {
            continue;
        }
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
