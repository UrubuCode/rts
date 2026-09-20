//! What a specifier NAMES, and what a path is called once it has been resolved.
//!
//! # Why this is beside the loader rather than inside it
//!
//! The loader answers "which files is this program made of"; this answers
//! "which file is THIS". The second question is asked from three places the
//! first never reaches: the runtime, when a `require("./x")` or an
//! `import("./x")` asks while the program is running; the object path, which
//! records every answer so an AOT binary can carry them where it has no disk;
//! and the walk itself, deciding what to read next.
//!
//! One answer for all three, which is the whole point. `rts-core`'s
//! `dynamic_module` header has what a second one costs: `createRequire`
//! reproduced this rule, said in its own comment that it had to match
//! "exactly", and stopped matching the day [`plain`] started stripping
//! Windows's verbatim prefix.
//!
//! It moved out of `mod.rs` because that file passed this crate's 500-line
//! ceiling, and this is the cohesive half: everything here is about a PATH and
//! nothing here reads a program.

use std::path::{Path, PathBuf};

/// Whether a specifier names a file rather than something the host provides.
pub(super) fn is_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

/// What a dynamic `import()` specifier means, from the module that wrote it.
///
/// # Why the runtime asks instead of the tree being rewritten
///
/// A static specifier is rewritten in place ([`super::rewrite`]), because it is written
/// in the grammar and the loader has already resolved it. A dynamic one may be
/// COMPUTED — `import("./" + name)` — so there is nothing in the tree to
/// rewrite, and the question only exists once the program is running. This is
/// the same resolution, reached from there: `rts-core` holds the hook and this
/// fills it, exactly as it does for compiling source.
///
/// `None` for anything that does not name a file, which leaves the specifier
/// as the program wrote it — the rule the loader applies to `node:fs` and to a
/// bare name, stated once and applied in both directions.
pub(crate) fn resolve_specifier(from: &str, specifier: &str) -> Option<String> {
    // The SAME question the loader's walk asked, through the same map, so a
    // name that named a file at load time still names that file at run time.
    let from = Path::new(from);
    let found = super::tsconfig::with_active(|aliases| resolve_written(from, specifier, aliases))?;
    Some(found.display().to_string())
}

/// A path as the `file:` URL `import.meta.url` answers.
///
/// # Why this is spelled out rather than taken from a crate
///
/// Because the one rule that matters here is small and the failure mode of
/// getting it wrong is invisible: Node and Bun both answer a `file:` URL, and a
/// program tests it with `startsWith("file:")`. What a full URL crate would add
/// is percent-encoding of the characters a path may hold, which is real and is
/// why the encoding below is named as partial rather than claimed complete: a
/// space becomes `%20`, and anything else is left as written.
pub(super) fn file_url(path: &Path) -> String {
    let text = path.display().to_string();
    // A Windows path is `C:\a\b`, which is `file:///C:/a/b`. A POSIX one
    // already starts with `/`, so the third slash is the path's own.
    let slashed = text.replace('\\', "/").replace(' ', "%20");
    match slashed.starts_with('/') {
        true => format!("file://{slashed}"),
        false => format!("file:///{slashed}"),
    }
}

/// The candidates for a specifier written without an extension, or a directory.
///
/// # Why this became an ORDER, and what the old rule was protecting
///
/// The rule was "`./x`, and then `./x.ts` — one candidate, not a cascade",
/// because a resolver that tries several picks a file the program did not name,
/// and which one it picked is invisible until two of them exist. That fear is
/// real and the answer to it is not a shorter list, it is a WRITTEN one: the
/// order below is the rule, a program with both `x.ts` and `x.js` gets `x.ts`,
/// and that sentence is testable where "the resolver decides" is not.
///
/// An intermediate version refused an ambiguity outright — two candidates, no
/// answer — and it is worth recording why that lost, because it looked stricter
/// and therefore safer. Node's own `test/common/` holds an `index.js` and an
/// `index.mjs`, and so does a large share of real packages: those are not two
/// spellings of one module, they are the two entry points a package publishes.
/// Refusing them refuses the corpus this exists to run, and calls a deliberate
/// pair an accident.
///
/// `.ts` leads because this repository's own suite is TypeScript and every
/// relative import in it omits the extension. `.js` follows because that is
/// what `require("./x")` means everywhere else.
pub(super) fn extended(base: &Path, specifier: &str) -> Option<PathBuf> {
    let named = base.join(specifier);
    // A directory names the file inside it, which is what `require("./lib")`
    // means in the corpus this serves. Tried only when the name IS a directory,
    // so it can never collide with the file candidates below.
    let candidates: Vec<PathBuf> = match named.is_dir() {
        true => ["index.ts", "index.js", "index.cjs", "index.mjs"]
            .iter()
            .map(|name| named.join(name))
            .collect(),
        false => ["ts", "js", "cjs", "mjs"]
            .iter()
            .map(|extension| base.join(format!("{specifier}.{extension}")))
            .collect(),
    };
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// A canonical path with Windows's verbatim prefix taken off.
///
/// # Why every canonicalisation here goes through this
///
/// `Path::canonicalize` answers the extended-length form on Windows —
/// `\\?\C:\a\b` — which is a real path and which nothing outside the OS expects
/// to see. It leaks into three things a PROGRAM reads: the specifier a module is
/// registered under, `__filename`/`__dirname`, and `import.meta.url`.
///
/// The third is what made this a defect rather than an ugliness. [`file_url`]
/// turns the backslashes into slashes, so the prefix became `file:////?/C:/…` —
/// an empty authority, a path of `//`, and everything after the `?` read as a
/// QUERY. `new URL(import.meta.url).pathname` answered `"/"`, and
/// `module.createRequire(import.meta.url)` answered `undefined` because it could
/// not get a file path back out of it. Measured 2026-08-24 against Node's own
/// suite: 49 files died on `require is not a function`, every one of them
/// through `common/index.mjs`, whose first line is exactly that call.
///
/// Stripped HERE, at the one place a canonical path is made, rather than at each
/// of the three readers — three strippers are three chances for one of them to
/// be forgotten, which is how this arrived in the first place.
pub(super) fn plain(path: PathBuf) -> PathBuf {
    let text = path.display().to_string();
    // `\\?\UNC\server\share` is a UNC path, and its plain form keeps the two
    // leading backslashes: dropping the whole prefix would name a local
    // directory called `server`.
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// One file, one spelling: canonicalised, then stripped of Windows's
/// verbatim prefix.
///
/// Both branches of [`resolve_written`] end here, and that is the point. The
/// loader keys a module by the string this answers, so two written names for
/// one file that came back as two strings would be two modules with two
/// namespaces — the failure the design's §5 exists to prevent. A `paths`
/// target is allowed to escape the project (`"@lib/*": ["../../shared/*"]`),
/// so a joined alias path really does keep a `..` that the relative spelling
/// of the same file does not, and `tests/import_alias.rs` failed on exactly
/// that before this was shared.
fn settled(path: PathBuf) -> PathBuf {
    plain(path.canonicalize().unwrap_or(path))
}

/// The path a relative specifier names, from the file that wrote it.
pub(super) fn resolve(from: &Path, specifier: &str) -> PathBuf {
    let base = from.parent().unwrap_or(Path::new("."));
    let named = base.join(specifier);
    // What the program wrote, and then the one candidate: `./x` before `./x.ts`.
    // Written this way round so a file that genuinely has no extension still
    // wins over a `.ts` beside it — the program named that one.
    let joined = match named.is_file() {
        true => named,
        false => extended(base, specifier).unwrap_or(named),
    };
    // Canonicalised so that `./a.ts` and `../dir/a.ts` are the SAME module. Two
    // spellings of one file compiled twice would run its side effects twice and
    // give it two namespaces, and `import { x } from` each would answer two
    // different `x`.
    settled(joined)
}

/// Whether a specifier names something the HOST provides rather than a file.
///
/// A `:` before any `/` is a scheme: `node:fs`, `rts:egui`. Such a name is
/// never a path, and asking the disk about it is not merely wasteful — with a
/// `baseUrl` set, a directory called `node` beside the config would answer,
/// and `node:fs` would silently become a user's file. That failure compiles
/// and lies, which is the one this crate's rule 1 names.
///
/// A Windows absolute specifier (`C:/x`) has the same shape and gets the same
/// answer, which is also what it gets today: [`is_relative`] is false for it.
pub fn names_the_host(specifier: &str) -> bool {
    match specifier.find(':') {
        None => false,
        Some(colon) => !specifier[..colon].contains('/'),
    }
}

/// Whether this specifier names a FILE, and which.
///
/// The one question, asked by the loader's walk, by `rewrite`, and by the
/// runtime resolver. It replaced a bare [`is_relative`] at each of those sites
/// so that a second thing never learns what a path is — the header of
/// `rts_core::entry::dynamic_module` records what the last copy cost.
///
/// `None` keeps its established meaning: not a file, so the text is used as
/// written and the host provides it by name.
pub fn resolve_written(from: &Path, specifier: &str, aliases: &super::Aliases) -> Option<PathBuf> {
    if is_relative(specifier) {
        return Some(resolve(from, specifier));
    }
    if names_the_host(specifier) {
        return None;
    }
    // Every candidate the map offers, in the map's order, and the first that
    // is a real file. `extended` is what "a real file" means here — extension
    // and `index.*` — and it is called rather than reproduced.
    for candidate in aliases.candidates(specifier) {
        // A target may already be written with its extension
        // (`"@/one": ["./exact/one.ts"]`), and `extended` assumes the opposite
        // — handed "one.ts" it tries "one.ts.ts" next. Checked literally FIRST,
        // the same order `resolve` already uses for a relative specifier, so an
        // exact target is not run through a rule written for an extension-less
        // one.
        if candidate.is_file() {
            return Some(settled(candidate));
        }
        let parent = candidate.parent()?;
        let name = candidate.file_name()?.to_str()?;
        if let Some(found) = extended(parent, name) {
            return Some(settled(found));
        }
    }
    None
}
