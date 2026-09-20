//! The walk: which files a program is made of, and in what order they run.
//!
//! Split out of `mod.rs` when that file passed this crate's 500-line ceiling
//! (rule 6), along the seam that was already there. Everything here TRAVERSES
//! — it reads a file, asks what that file names, and follows the answer. What
//! stayed behind is what the traversal produces and what is done to the result
//! afterwards: `Loaded`, `Graph`, `rewrite` and `front_end`.
//!
//! Why the order is post-order, and why a cycle is refused rather than linked,
//! are on [`load`] itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rts_codegen::names::Names;
use rts_codegen::parse::parse_module;
use rts_codegen::syntax::ModuleItem;

use super::resolve::{is_relative, plain};
use super::{Aliases, Loaded, resolve_written, tsconfig};
use crate::link::HostError;


/// The specifiers one module writes that `keep` accepted, each with what
/// `keep` LEARNED about it, in source order.
///
/// The predicate is a parameter, and the walk below is written once, for the
/// same reason the `Wanted` forms are a parameter: two walks over one tree are
/// two chances for a node to be visited by one and skipped by the other. Two
/// callers ask two different questions of it — see the two wrappers.
///
/// `keep` answers `Option<T>` rather than `bool` so that what it worked out in
/// order to decide comes BACK with the specifier. The graph walk's predicate
/// has to resolve a path to know whether the specifier names a file at all,
/// and a `bool` would throw that path away and force [`visit`] to ask for it a
/// second time. The caller that does not care passes `T = ()`.
///
/// The tree is thrown away: this wants the specifiers and nothing else, and a
/// `Name` is an index into a table this parse owns and no other compilation
/// issued.
fn specifiers_naming_files<T>(
    source: &str,
    keep: impl Fn(&str) -> Option<T>,
) -> Result<Vec<(String, T)>, String> {
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
        if let Some(learned) = keep(&specifier) {
            found.push((specifier, learned));
        }
    }
    // And what a `import("./x")` names. It is the same question — which files
    // is this program made of — and a module reached only that way still has to
    // be compiled into the one compilation, for the reason at the top of this
    // file: a namespace built in another region is one the importer cannot
    // touch. The walk is `rts-codegen`'s because the tree is.
    //
    // The divergence this leaves is stated rather than hidden: such a module is
    // EVALUATED with the rest of the graph, dependencies first, where the
    // language evaluates it at the `import()` call. A program whose dynamic
    // import is behind a condition runs its body anyway.
    //
    // And a `require("./x")` names one too, for the identical reason: a module
    // reached only that way is still a file this program is made of, and it has
    // to be in the ONE compilation or its exports are cells the requiring module
    // cannot touch. Asked of the SAME walk, which is why the forms are a
    // parameter rather than a second function — two walks over one tree are two
    // chances for a node to be visited by one and skipped by the other.
    let wanted = rts_codegen::emit::Wanted {
        dynamic_import: true,
        require: Some(scratch.intern("require")),
        dynamic_code: None,
    };
    for specifier in rts_codegen::emit::specifiers(&parsed.body, wanted) {
        if let Some(learned) = keep(&specifier) {
            found.push((specifier, learned));
        }
    }
    Ok(found)
}

/// The RELATIVE specifiers one module writes — and only those.
///
/// Deliberately blind to aliases. Its caller is the `rts run https://…`
/// mirror, which fetches a remote program's files before anything local is
/// consulted: a remote `@/secret` resolving against THIS machine's
/// `tsconfig.json` would read local files on a remote program's behalf. The
/// mirror also holds a URL rather than a path, so there is no referrer to
/// resolve an alias from even if it were wanted — which is why this keeps its
/// signature instead of gaining a `from`.
pub fn relative_imports(source: &str) -> Result<Vec<String>, String> {
    // Nothing is learned deciding this — `is_relative` reads the first two
    // characters — so the carried value is the unit and this drops it.
    let found = specifiers_naming_files(source, |specifier| is_relative(specifier).then_some(()))?;
    Ok(found.into_iter().map(|(specifier, ())| specifier).collect())
}

/// Which FILES this module is made of, aliases included: each specifier as
/// written, paired with the path it resolved to.
///
/// What the graph walk asks. The answer comes from [`resolve_written`], the
/// one question, so nothing here learns a second time what a path is.
///
/// The PATH is returned and not just the fact that there was one, because
/// deciding whether a specifier names a file IS resolving it: there is no
/// cheaper test. Handing the answer back is what lets [`visit`] follow the
/// resolution this function accepted instead of asking again for a second one.
pub(crate) fn imported_files(source: &str, from: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    specifiers_naming_files(source, |specifier| {
        tsconfig::with_active(|aliases| resolve_written(from, specifier, aliases))
    })
}

/// Whether this source names any FILE: a relative specifier, or one an ALIAS
/// resolves — the question [`load`] would answer by walking, asked before any
/// walking happens.
///
/// It exists because the answer was being GUESSED elsewhere. A caller that has
/// to choose between compiling a graph and compiling one file alone used a
/// substring test for `from "./"` and its three spellings; an aliased
/// specifier contains none of them, so a program whose every import is an
/// alias was compiled alone and died at run time on `cannot resolve module
/// "@/…"`. `rts_core::entry::dynamic_module`'s header records what a second
/// implementation of "what is a path" costs; this is the first implementation,
/// exported so there need not be a second.
///
/// # It INSTALLS the map, and does not merely borrow it
///
/// The question is asked BEFORE [`load`] runs, so nothing has installed this
/// entry's `Aliases` yet and this has to discover them itself. Having
/// discovered them it installs them, for two reasons: a discovery thrown away
/// is one [`load`] immediately repeats — it reads the same `tsconfig.json`
/// chain off disk — and [`load`] installs unconditionally at its top, so an
/// install left here can never be read stale by a graph compile.
///
/// The one path that must NOT inherit it is the entry-less compile, which has
/// no file and therefore no project; that path already forgets the map by name
/// ([`super::forget_aliases`], called from [`crate::run::compile_for`]), so it
/// is unaffected by an install here. That ordering is the whole safety
/// argument, and it is why this installs rather than restoring what was there.
///
/// `Err` is a parse failure in `source`, reported rather than swallowed: a
/// caller that cannot parse the program is about to fail compiling it anyway,
/// and a silent `false` would send it down the single-file path and rename the
/// failure.
pub fn names_any_file(source: &str, entry: &Path) -> Result<bool, String> {
    tsconfig::install(Aliases::discover(entry));
    Ok(!imported_files(source, entry)?.is_empty())
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
    // The map for this program, found ONCE and installed before the walk
    // begins. Every resolution below reads this one answer, and so does every
    // dynamic one while the program runs — spec §5's "one map per program".
    tsconfig::install(Aliases::discover(entry));
    let start = plain(entry.canonicalize().unwrap_or_else(|_| entry.to_owned()));
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
    let mut resolutions = Vec::new();
    // The path comes back WITH the specifier: deciding that a specifier names a
    // file is resolving it, and this is the resolution that decision was made
    // on rather than a second one asked for here. The loader keys a module by
    // that path, so the one place it is produced is the one place it can be.
    for (specifier, resolved) in imported_files(&source, path)
        .map_err(|error| HostError::Parse(format!("{}: {error}", path.display())))?
    {
        // Kept, not just followed: a `require("./x")` asks again at run time,
        // and the answer is this one. See [`Loaded::resolutions`].
        resolutions.push((specifier.clone(), resolved.display().to_string()));
        visit(&resolved, ordered, state)?;
    }

    state.insert(path.to_owned(), Mark::Done);
    ordered.push(Loaded {
        specifier: path.display().to_string(),
        path: path.to_owned(),
        resolutions,
        // A fachada do DOM entra aqui pelo mesmo critério do caminho de ficheiro
        // único — `crate::run::with_dom_facade` é o único sítio onde a decisão
        // está escrita, e este passa a chamá-lo em vez de a repetir.
        //
        // Faltava, e o que a falta impedia é maior do que parece: um programa
        // com UMA linha de `import` compila como GRAFO, e este caminho nunca
        // injetava o prelude. Ou seja NENHUM programa que importasse seja o que
        // fosse — `node:fs`, `rts:egui` — podia usar `parseDocument`, e a falha
        // era `ReferenceError: parseDocument is not defined`, que aponta para o
        // programa quando o que faltava era o prelude.
        //
        // Por FICHEIRO e não só na entrada, que é o mesmo que o outro caminho
        // faz: quem menciona a fachada recebe-a. Dois módulos que a mencionem
        // ficam com classes distintas — um `instanceof` entre eles responderia
        // falso — e isso está por resolver; o que não estava era poder usá-la
        // de todo.
        source: crate::run::with_dom_facade(&source),
    });
    Ok(())
}
