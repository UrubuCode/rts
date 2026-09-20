//! What a `tsconfig.json` says about where a written name lives.
//!
//! # Why the host reads the file rather than being handed the map
//!
//! Because every caller that compiles a graph would otherwise have to find and
//! parse it: `rts run`, `rts compile`, and every test that calls
//! `compile_graph` directly. Two of those are in another crate. Finding the
//! file would then be known in two places, and where the config lives is
//! exactly the kind of fact that drifts — the loader's own header records what
//! a second copy of a resolution rule cost.
//!
//! # What is deliberately NOT here
//!
//! Type checking, and every field but three. `extends`, `compilerOptions.
//! baseUrl` and `compilerOptions.paths` are read; the rest is ignored, and
//! ignoring it is not a promise to honour it later.

use std::path::{Path, PathBuf};

/// One `paths` entry, with its wildcard split out and its base remembered.
struct Pattern {
    /// The text before the `*`, or the whole key when there is none.
    prefix: String,
    /// The text after the `*`. Empty when the key ends in `*`.
    suffix: String,
    /// Whether the key held a `*` at all. An exact key matches whole.
    wildcard: bool,
    /// The substitutions, in the order the file wrote them, each already
    /// joined to the directory of the `tsconfig.json` that WROTE it.
    targets: Vec<PathBuf>,
}

/// Where a written name may live, for one program.
///
/// Empty is the answer when no `tsconfig.json` was found, and an empty map
/// resolves nothing — which is what makes "no config, no change" a property of
/// the type rather than of a branch someone has to remember.
pub struct Aliases {
    patterns: Vec<Pattern>,
    base_url: Option<PathBuf>,
}

impl Aliases {
    /// No config: nothing is a candidate.
    pub fn none() -> Aliases {
        Aliases { patterns: Vec::new(), base_url: None }
    }

    /// Whether nothing was found — no `paths`, no `baseUrl`.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.base_url.is_none()
    }

    /// Reads the nearest `tsconfig.json` at or above `entry`'s directory.
    ///
    /// Answers [`Aliases::none`] for every way this can fail — absent,
    /// unreadable, malformed. A config file that cannot be read is not a
    /// reason to refuse to compile a program that may not use aliases at all,
    /// and the failure surfaces as the import not resolving, which names the
    /// import.
    pub fn discover(entry: &Path) -> Aliases {
        let mut directory = match entry.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Aliases::none(),
        };
        loop {
            let candidate = directory.join("tsconfig.json");
            if candidate.is_file() {
                return read(&candidate, 0).unwrap_or_else(Aliases::none);
            }
            match directory.parent() {
                Some(parent) => directory = parent.to_path_buf(),
                None => return Aliases::none(),
            }
        }
    }

    /// The paths a specifier MIGHT name, in the order they must be tried.
    ///
    /// Touches no disk. The caller asks the disk, because the caller owns what
    /// a file is — extensions and `index.*` — and that is `resolve::extended`.
    pub fn candidates(&self, specifier: &str) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Some(pattern) = self.best_match(specifier) {
            let stem = &specifier[pattern.prefix.len()..specifier.len() - pattern.suffix.len()];
            for target in &pattern.targets {
                found.push(without_current_dir(substitute(target, stem, pattern.wildcard)));
            }
        }
        // `baseUrl` is tried AFTER every `paths` target, never instead of them:
        // a pattern that matches but whose targets are all missing still falls
        // through to here, which is `tsc`'s order. The caller walks this list and
        // stops at the first that exists, so an extra candidate costs one probe.
        if let Some(base) = &self.base_url {
            found.push(without_current_dir(base.join(specifier)));
        }
        found
    }

    /// The pattern that wins: an exact key first, then the longest literal
    /// prefix. NOT source order — spec §9 point 3, and the one that is got
    /// wrong silently, because source order agrees with it until two patterns
    /// overlap.
    fn best_match(&self, specifier: &str) -> Option<&Pattern> {
        let mut best: Option<&Pattern> = None;
        for pattern in &self.patterns {
            let matches = match pattern.wildcard {
                false => specifier == pattern.prefix,
                true => {
                    specifier.len() >= pattern.prefix.len() + pattern.suffix.len()
                        && specifier.starts_with(&pattern.prefix)
                        && specifier.ends_with(&pattern.suffix)
                }
            };
            if !matches {
                continue;
            }
            if !pattern.wildcard {
                return Some(pattern);
            }
            let better = match best {
                None => true,
                Some(current) => pattern.prefix.len() > current.prefix.len(),
            };
            if better {
                best = Some(pattern);
            }
        }
        best
    }
}

/// A target with its `*` replaced by what the specifier had there.
fn substitute(target: &Path, stem: &str, wildcard: bool) -> PathBuf {
    if !wildcard {
        return target.to_path_buf();
    }
    let text = target.to_string_lossy().replace('*', stem);
    PathBuf::from(text)
}

/// A path with its `.` components removed.
///
/// `directory.join("./src/x")` and `directory.join(".")` both KEEP a
/// literal `CurDir`, so `dir/./x.ts` and `dir/x.ts` are two strings
/// naming one file. The loader keys a module by its resolved path, so
/// leaving them would make a module reached through an alias and the
/// same module reached relatively into two modules with two namespaces.
///
/// Not `canonicalize`: that answers a `\\?\` path on Windows — the
/// reason `resolve::plain` exists — and resolves symlinks, and the
/// relative path does neither. Normalising only this side would create
/// the divergence it is meant to remove. A `.` is a no-op component by
/// definition, so dropping it cannot change which file is named.
fn without_current_dir(path: PathBuf) -> PathBuf {
    use std::path::Component;
    let mut built = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            other => built.push(other),
        }
    }
    built
}

/// Reads one file and everything it extends.
///
/// `depth` refuses a cycle by exhaustion rather than by bookkeeping: a chain
/// of configs is a handful deep in every real project, and a visited-set here
/// would be state that only a malformed project ever reads.
fn read(path: &Path, depth: usize) -> Option<Aliases> {
    if depth > 16 {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&crate::jsonc::strip(&raw)).ok()?;
    let directory = path.parent()?;

    // The parent first, so the child can override key by key.
    let mut aliases = match value.get("extends").and_then(|one| one.as_str()) {
        Some(parent) => {
            let target = directory.join(parent);
            let target = match target.extension() {
                Some(_) => target,
                None => target.with_extension("json"),
            };
            read(&target, depth + 1).unwrap_or_else(Aliases::none)
        }
        None => Aliases::none(),
    };

    let options = value.get("compilerOptions");
    if let Some(base) = options.and_then(|one| one.get("baseUrl")).and_then(|one| one.as_str()) {
        aliases.base_url = Some(directory.join(base));
    }
    if let Some(paths) = options.and_then(|one| one.get("paths")).and_then(|one| one.as_object()) {
        for (key, targets) in paths {
            let listed: Vec<PathBuf> = targets
                .as_array()
                .map(|all| {
                    all.iter()
                        .filter_map(|one| one.as_str())
                        .map(|one| directory.join(one))
                        .collect()
                })
                .unwrap_or_default();
            // A key the child redefines replaces the parent's entirely, which
            // is what `tsc` does: `paths` is merged by KEY, not by target.
            aliases.patterns.retain(|existing| !same_key(existing, key));
            aliases.patterns.push(split(key, listed));
        }
    }
    Some(aliases)
}

/// Whether a stored pattern came from this written key.
fn same_key(pattern: &Pattern, key: &str) -> bool {
    let rebuilt = match pattern.wildcard {
        true => format!("{}*{}", pattern.prefix, pattern.suffix),
        false => pattern.prefix.clone(),
    };
    rebuilt == key
}

/// A written key, split at its wildcard.
///
/// More than one `*` is an error in `tsc`. It is refused here by being treated
/// as no wildcard at all, so such a key matches only itself and never
/// silently resolves something else — spec §9 point 1.
fn split(key: &str, targets: Vec<PathBuf>) -> Pattern {
    match key.split('*').count() {
        2 => {
            let mut halves = key.splitn(2, '*');
            Pattern {
                prefix: halves.next().unwrap_or_default().to_string(),
                suffix: halves.next().unwrap_or_default().to_string(),
                wildcard: true,
                targets,
            }
        }
        _ => Pattern { prefix: key.to_string(), suffix: String::new(), wildcard: false, targets },
    }
}

thread_local! {
    /// The map for the program this thread is loading or running.
    ///
    /// A thread local and not a `OnceLock` because `rts_core::entry::Resolver`
    /// is `fn(&str, &str) -> Option<String>` — a bare pointer with nowhere to
    /// carry a map — so the map has to be reachable without being passed, and
    /// a process-wide one is shared by every test the harness runs in
    /// parallel. The first test to install would decide for all of them.
    ///
    /// What this does NOT survive: a program whose load and whose run are on
    /// different threads. `import_alias.rs`'s dynamic-import test is what
    /// fails if that ever becomes true, rather than a program silently
    /// failing to resolve.
    static ACTIVE: std::cell::RefCell<Aliases> = std::cell::RefCell::new(Aliases::none());
}

/// Makes this the map for this thread, replacing any previous one.
///
/// # How long it lives
///
/// From the `load` that installs it until the NEXT `load` on this thread, and
/// no shorter. It is deliberately not scoped to the load, because the runtime
/// resolver has to read it while the program runs — which is after `load`
/// returned. The consequence is that a compile with no entry file has no
/// project and therefore no map, and must say so instead of inheriting: see
/// [`crate::run::compile_for`], which installs [`Aliases::none`] through
/// [`super::forget_aliases`].
pub(crate) fn install(aliases: Aliases) {
    ACTIVE.with(|slot| *slot.borrow_mut() = aliases);
}

/// Asks the current map a question, without cloning it.
pub(crate) fn with_active<T>(ask: impl FnOnce(&Aliases) -> T) -> T {
    ACTIVE.with(|slot| ask(&slot.borrow()))
}
