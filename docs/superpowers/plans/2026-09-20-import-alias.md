# Import Alias Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A specifier that is neither relative nor host-provided may name a file, when a `tsconfig.json` says which one.

**Architecture:** One function — `resolve_written` — answers "does this name a file, and which", replacing the six `is_relative` branches in `graph/mod.rs` and the runtime resolver hook. A new `graph/tsconfig.rs` finds and parses the config; `resolve.rs` keeps owning what a path is. Nothing else learns what a path is, which is the property `rts_core::entry::dynamic_module`'s header records as having been broken once.

**Tech Stack:** Rust, `rts-host` crate. `serde_json` (new to this crate, already in the workspace via `rts-cli`).

**Spec:** `docs/superpowers/specs/2026-09-20-import-alias-design.md` — read it before Task 1. The plan argues from it.

## Global Constraints

- **RULE 0:** `crates/rts-host/README.md` (6 rules) is binding. Read in full before editing the crate.
- **Rule 1 — this crate holds no semantics.** Resolution is "which file is this", not what JavaScript means. Nothing in this work may decide a language question.
- **Rule 4 — both destinations, or neither.** Any JIT/AOT difference is stated and is about the destination.
- **Rule 5 — a test here runs the program.** No unit test stands in for an end-to-end one where the behaviour is observable by running.
- **Rule 6 — files stop at 500 lines.** `graph/mod.rs` is 429 and `resolve.rs` is 162 today. Check with `wc -l` before each commit.
- **The no-config guarantee:** with no `tsconfig.json` found, resolution is byte for byte what it is today. Task 4 makes this a test; every later task must keep it passing.
- **Commit message language:** Portuguese, matching this repository's history. No accents in commit subjects (the existing log is ASCII).

---

### Task 1: `strip_json_comments` moves down, and learns block comments

`tsconfig.json` is JSONC. The stripper that exists handles `//` only, and lives in `rts-cli`, which **depends on** `rts-host` — so it cannot be called from the host. It moves to the host and `rts-cli` calls it there. One implementation.

**Files:**
- Create: `crates/rts-host/src/jsonc.rs`
- Modify: `crates/rts-host/src/lib.rs` (add `pub mod jsonc;`)
- Modify: `crates/rts-cli/src/manifest.rs:19-60` (delete the function, re-export)
- Modify: `crates/rts-host/Cargo.toml` (add `serde_json = "1.0"`)
- Test: `crates/rts-host/tests/jsonc.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `rts_host::jsonc::strip(input: &str) -> String`.

- [ ] **Step 1: Write the failing test**

```rust
//! JSONC is what `tsconfig.json` and `package.json` are written in.

use rts_host::jsonc::strip;

#[test]
fn a_line_comment_goes_and_the_newline_stays() {
    assert_eq!(strip("{\n  // a\n  \"x\": 1\n}"), "{\n  \n  \"x\": 1\n}");
}

#[test]
fn a_block_comment_goes() {
    assert_eq!(strip("{/* a */\"x\": 1}"), "{\"x\": 1}");
}

/// The case the `//`-only version got wrong, and the reason this moved: a
/// `tsconfig.json` written by `tsc --init` is full of block comments.
#[test]
fn a_block_comment_spanning_lines_goes() {
    assert_eq!(strip("{\n/* a\n b */\n\"x\": 1}"), "{\n\n\"x\": 1}");
}

/// A trailing comma is legal in JSONC and fatal to `serde_json`.
#[test]
fn a_trailing_comma_goes_from_an_object_and_an_array() {
    assert_eq!(strip("{\"x\": [1, 2,],}"), "{\"x\": [1, 2]}");
}

/// What a stripper must never do: a comment marker inside a string is text.
#[test]
fn what_is_inside_a_string_is_left_alone() {
    assert_eq!(strip("{\"x\": \"a // b /* c */\"}"), "{\"x\": \"a // b /* c */\"}");
    assert_eq!(strip("{\"x\": \"a,\"}"), "{\"x\": \"a,\"}");
}

/// An escaped quote does not end the string, so what follows is still text.
#[test]
fn an_escaped_quote_does_not_end_the_string() {
    assert_eq!(strip("{\"x\": \"a\\\" // b\"}"), "{\"x\": \"a\\\" // b\"}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rts-host --test jsonc`
Expected: FAIL — `unresolved import rts_host::jsonc`.

- [ ] **Step 3: Write minimal implementation**

Create `crates/rts-host/src/jsonc.rs`:

```rust
//! JSONC to JSON: what a hand-written config file is, made into what a parser
//! accepts.
//!
//! # Why it lives here and not beside its first caller
//!
//! It was written in `rts-cli`, for `package.json`. `rts-cli` DEPENDS on this
//! crate, so the loader could not call it, and the loader needs it for
//! `tsconfig.json`. The choice was to copy it down or move it down; a copy is
//! two answers to "what is a comment", and the two would drift the first time
//! one of them learned a form the other did not — which is exactly what this
//! version does to the original by learning block comments.
//!
//! # What it does NOT do
//!
//! It is not a JSON parser and does not validate. It removes comments and
//! trailing commas so that a real parser can read what is left. A malformed
//! file is the parser's error to report, with the parser's message.

/// JSONC to JSON: comments and trailing commas removed, everything else kept.
///
/// Byte offsets are NOT preserved — a comment becomes nothing, not spaces —
/// so an error position from the parser indexes the stripped text. No caller
/// reports positions today; one that wants to must keep the original instead
/// of asking for it back from here.
pub fn strip(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                output.push(ch);
            }
            '/' if matches!(chars.peek(), Some('/')) => {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            // A block comment keeps the newlines it spanned, so a line number
            // computed on the stripped text still means the same line.
            '/' if matches!(chars.peek(), Some('*')) => {
                let _ = chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                    }
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            ',' => {
                // A comma is trailing when the next thing that is not space or
                // a comment closes the container. Decided by looking, because
                // the alternative is a second pass that has to agree with this
                // one about what a comment is.
                let mut lookahead = chars.clone();
                let mut next_real = None;
                while let Some(peeked) = lookahead.next() {
                    match peeked {
                        c if c.is_whitespace() => continue,
                        '/' if matches!(lookahead.peek(), Some('/')) => {
                            for skipped in lookahead.by_ref() {
                                if skipped == '\n' {
                                    break;
                                }
                            }
                        }
                        '/' if matches!(lookahead.peek(), Some('*')) => {
                            let _ = lookahead.next();
                            let mut previous = '\0';
                            for skipped in lookahead.by_ref() {
                                if previous == '*' && skipped == '/' {
                                    break;
                                }
                                previous = skipped;
                            }
                        }
                        other => {
                            next_real = Some(other);
                            break;
                        }
                    }
                }
                if !matches!(next_real, Some('}') | Some(']')) {
                    output.push(ch);
                }
            }
            other => output.push(other),
        }
    }

    output
}
```

Add to `crates/rts-host/src/lib.rs`, beside the other module declarations:

```rust
pub mod jsonc;
```

Add to `crates/rts-host/Cargo.toml`, in `[dependencies]`:

```toml
# `tsconfig.json`, for import aliases. This crate's only other external
# dependency is `libc`, and that is deliberate — the entry is here because the
# loader must read a config file and hand-rolling a JSON parser to avoid a
# dependency already in this workspace's lockfile is the worse trade.
serde_json = "1.0"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rts-host --test jsonc`
Expected: PASS, 6 tests.

- [ ] **Step 5: Point `rts-cli` at the moved function**

In `crates/rts-cli/src/manifest.rs`, delete the `strip_json_comments` body (lines 19-60) and replace with:

```rust
/// Strip JSONC down to JSON.
///
/// Moved to `rts-host` so the module loader can call it for `tsconfig.json`:
/// `rts-cli` depends on `rts-host` and not the other way round, so the shared
/// answer has to live in the lower crate. Kept as a name here because callers
/// in this crate spell it this way.
pub use rts_host::jsonc::strip as strip_json_comments;
```

- [ ] **Step 6: Verify the whole workspace still builds and its tests pass**

Run: `cargo build --workspace --tests 2>&1 | tail -20`
Expected: the two pre-existing broken targets named in `crates/rts-host/Cargo.toml`'s feature comment (`rts-codegen` test "language", `rts-runtime`) and **no others**. If a third appears, this task caused it.

Run: `cargo test -p rts-cli 2>&1 | tail -20`
Expected: PASS — `package.json` parsing still works through the moved function.

- [ ] **Step 7: Commit**

```bash
git add crates/rts-host/src/jsonc.rs crates/rts-host/src/lib.rs \
        crates/rts-host/Cargo.toml crates/rts-cli/src/manifest.rs \
        crates/rts-host/tests/jsonc.rs
git commit -m "refactor(host): o limpador de JSONC desce para quem precisa dele primeiro"
```

---

### Task 2: reading a `tsconfig.json` into a map

**Files:**
- Create: `crates/rts-host/src/graph/tsconfig.rs`
- Modify: `crates/rts-host/src/graph/mod.rs` (add `mod tsconfig;` beside `mod resolve;`)
- Test: `crates/rts-host/tests/tsconfig_read.rs`

**Interfaces:**
- Consumes: `rts_host::jsonc::strip` (Task 1).
- Produces:
  - `pub(crate) struct Aliases` with `pub(crate) fn none() -> Aliases`, `pub(crate) fn discover(entry: &Path) -> Aliases`, `pub(crate) fn is_empty(&self) -> bool`, and `pub(crate) fn candidates(&self, specifier: &str) -> Vec<PathBuf>`.
  - `Aliases::candidates` answers the bases to try, in order, WITHOUT touching the disk. Task 3 defines its ordering; Task 4 is what asks the disk.
- For the test to reach it, `graph/mod.rs` gains `pub use tsconfig::Aliases;` and `crates/rts-host/src/lib.rs` already re-exports `graph`. Verify with `grep -n "pub mod graph\|pub use graph" crates/rts-host/src/lib.rs` and add `pub mod graph;` only if absent.

- [ ] **Step 1: Write the failing test**

```rust
//! What a `tsconfig.json` says, read the way the loader will read it.

use rts_host::graph::Aliases;
use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_tsconfig_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// The guarantee the whole change rests on.
#[test]
fn no_tsconfig_anywhere_is_an_empty_map() {
    let dir = fixture("none", &[("src/app.ts", "export const x = 1;\n")]);
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(aliases.is_empty(), "no config found means no map, and today's behaviour");
}

#[test]
fn a_config_beside_the_entry_is_found() {
    let dir = fixture(
        "beside",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(!aliases.is_empty());
    let found = aliases.candidates("@/engine/scene");
    assert_eq!(found, vec![dir.join("src").join("engine").join("scene")]);
}

/// Upward, like node and bun and tsc: the entry is deep and the config is not.
#[test]
fn a_config_above_the_entry_is_found() {
    let dir = fixture(
        "above",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/deep/deeper/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/deep/deeper/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("src").join("a")]);
}

/// §9 point 5 of the spec — the one most likely to be got wrong. A target is
/// relative to the file that WROTE it, which `extends` makes visible.
#[test]
fn extends_resolves_targets_against_the_file_that_wrote_them() {
    let dir = fixture(
        "extends",
        &[
            ("base/tsconfig.base.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./lib/*\"]}}}"),
            ("tsconfig.json", "{\"extends\":\"./base/tsconfig.base.json\"}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(
        aliases.candidates("@/thing"),
        vec![dir.join("base").join("lib").join("thing")],
        "the target was written in base/, so ./lib/ is base/lib/ and not the root's"
    );
}

/// A child's own `paths` wins over what it extends, key by key.
#[test]
fn a_child_overrides_the_key_it_redefines_and_keeps_the_rest() {
    let dir = fixture(
        "override",
        &[
            ("tsconfig.base.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./old/*\"],\"~/*\":[\"./keep/*\"]}}}"),
            ("tsconfig.json", "{\"extends\":\"./tsconfig.base.json\",\"compilerOptions\":{\"paths\":{\"@/*\":[\"./new/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("new").join("a")]);
    assert_eq!(aliases.candidates("~/a"), vec![dir.join("keep").join("a")]);
}

/// JSONC, because `tsc --init` writes it that way.
#[test]
fn comments_and_a_trailing_comma_do_not_stop_it() {
    let dir = fixture(
        "jsonc",
        &[
            (
                "tsconfig.json",
                "{\n // the project\n \"compilerOptions\": {\n /* aliases */\n \"paths\": {\"@/*\": [\"./src/*\"],},\n },\n}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("src").join("a")]);
}

/// A specifier nothing matches asks the disk for nothing.
#[test]
fn a_specifier_no_pattern_matches_has_no_candidates() {
    let dir = fixture(
        "nomatch",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(aliases.candidates("lodash").is_empty());
}

/// `baseUrl` alone, with no `paths`, makes every bare name a candidate.
#[test]
fn base_url_alone_makes_a_bare_name_a_candidate() {
    let dir = fixture(
        "baseurl",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\"./src\"}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("engine/scene"), vec![dir.join("src").join("engine").join("scene")]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rts-host --test tsconfig_read`
Expected: FAIL — `Aliases` not found.

- [ ] **Step 3: Write the implementation**

Create `crates/rts-host/src/graph/tsconfig.rs`:

```rust
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
    pub(crate) fn none() -> Aliases {
        Aliases { patterns: Vec::new(), base_url: None }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.base_url.is_none()
    }

    /// Reads the nearest `tsconfig.json` at or above `entry`'s directory.
    ///
    /// Answers [`Aliases::none`] for every way this can fail — absent,
    /// unreadable, malformed. A config file that cannot be read is not a
    /// reason to refuse to compile a program that may not use aliases at all,
    /// and the failure surfaces as the import not resolving, which names the
    /// import.
    pub(crate) fn discover(entry: &Path) -> Aliases {
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
    pub(crate) fn candidates(&self, specifier: &str) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Some(pattern) = self.best_match(specifier) {
            let stem = &specifier[pattern.prefix.len()..specifier.len() - pattern.suffix.len()];
            for target in &pattern.targets {
                found.push(substitute(target, stem, pattern.wildcard));
            }
        }
        // `baseUrl` is tried after every `paths` target, which is `tsc`'s own
        // order, and only for a name that is not a path.
        if let Some(base) = &self.base_url {
            found.push(base.join(specifier));
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
```

Add to `crates/rts-host/src/graph/mod.rs`, beside `mod resolve;`:

```rust
mod tsconfig;
pub use tsconfig::Aliases;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rts-host --test tsconfig_read`
Expected: PASS, 8 tests.

- [ ] **Step 5: Check the ceiling**

Run: `wc -l crates/rts-host/src/graph/tsconfig.rs crates/rts-host/src/graph/mod.rs`
Expected: both under 500. If `tsconfig.rs` is over, split `read`/`split`/`same_key` into `graph/tsconfig/read.rs` before committing — rule 6 is not negotiable.

- [ ] **Step 6: Commit**

```bash
git add crates/rts-host/src/graph/tsconfig.rs crates/rts-host/src/graph/mod.rs \
        crates/rts-host/tests/tsconfig_read.rs
git commit -m "feat(host): um tsconfig.json vira um mapa de onde um nome escrito mora"
```

---

### Task 3: the single question — `resolve_written`

**Files:**
- Modify: `crates/rts-host/src/graph/resolve.rs` (add the function)
- Test: `crates/rts-host/tests/resolve_written.rs`

**Interfaces:**
- Consumes: `Aliases::candidates` (Task 2), `resolve::extended`, `resolve::resolve` (existing).
- Produces: `pub(crate) fn resolve_written(from: &Path, specifier: &str, aliases: &Aliases) -> Option<PathBuf>`, and `pub(crate) fn names_the_host(specifier: &str) -> bool`.

- [ ] **Step 1: Write the failing test**

```rust
//! "Does this name a file, and which" — asked once, answered once.

use rts_host::graph::{names_the_host, resolve_written, Aliases};
use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_written_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Spec §4 row 2, and the reason it is not negotiable: a project whose
/// `baseUrl` contains a directory named `node` must not shadow `node:fs`.
#[test]
fn a_scheme_is_never_a_file_even_when_a_file_would_match() {
    let dir = fixture(
        "scheme",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            ("node/fs.ts", "export const x = 1;\n"),
            ("rts/egui.ts", "export const x = 1;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    assert!(names_the_host("node:fs"));
    assert!(names_the_host("rts:egui"));
    assert_eq!(resolve_written(&entry, "node:fs", &aliases), None, "the file must not win");
    assert_eq!(resolve_written(&entry, "rts:egui", &aliases), None);
}

/// A Windows absolute specifier has a scheme's shape and is left as written,
/// which is what happens today — spec §4.
#[test]
fn a_windows_absolute_specifier_is_left_as_written() {
    assert!(names_the_host("C:/somewhere/x"));
}

/// Spec §4 row 4: `baseUrl` missing does not fail, it falls through, or every
/// package import in a project with `baseUrl` would break.
#[test]
fn a_bare_name_base_url_cannot_find_falls_through() {
    let dir = fixture(
        "fallthrough",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    assert_eq!(resolve_written(&entry, "lodash", &aliases), None, "not a file, so the host's");
}

/// The relative path still works, and is still the first question asked.
#[test]
fn a_relative_specifier_resolves_as_it_always_did() {
    let dir = fixture(
        "relative",
        &[("src/app.ts", "export const x = 1;\n"), ("src/other.ts", "export const y = 2;\n")],
    );
    let entry = dir.join("src/app.ts");
    let found = resolve_written(&entry, "./other", &Aliases::none()).expect("resolved");
    assert_eq!(found, dir.join("src").join("other.ts"));
}

/// The alias resolves, and it goes through `extended` — the `.ts` was not
/// written and the caller did not add it.
#[test]
fn an_alias_resolves_through_the_existing_extension_rule() {
    let dir = fixture(
        "alias",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
            ("src/engine/scene.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine/scene", &aliases).expect("resolved");
    assert_eq!(found, dir.join("src").join("engine").join("scene.ts"));
}

/// An alias naming a directory takes its `index.*`, because `extended` does.
#[test]
fn an_alias_naming_a_directory_takes_its_index() {
    let dir = fixture(
        "aliasdir",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
            ("src/engine/index.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine", &aliases).expect("resolved");
    assert_eq!(found, dir.join("src").join("engine").join("index.ts"));
}

/// Spec §9 point 4: the list is tried in order and the first that EXISTS wins
/// — not the first that is written.
#[test]
fn a_list_of_targets_falls_through_to_the_one_that_exists() {
    let dir = fixture(
        "list",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./missing/*\",\"./real/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("real/thing.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/thing", &aliases).expect("resolved");
    assert_eq!(found, dir.join("real").join("thing.ts"));
}

/// Spec §9 point 3: longest literal prefix, NOT source order. Written with the
/// general pattern FIRST so source order would give the wrong answer.
#[test]
fn the_longest_prefix_wins_over_source_order() {
    let dir = fixture(
        "longest",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./general/*\"],\"@/engine/*\":[\"./specific/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("general/engine/scene.ts", "export const y = 2;\n"),
            ("specific/scene.ts", "export const z = 3;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine/scene", &aliases).expect("resolved");
    assert_eq!(found, dir.join("specific").join("scene.ts"), "@/engine/* is more specific");
}

/// An exact key beats a wildcard that also matches — spec §9 point 2.
#[test]
fn an_exact_key_beats_a_wildcard() {
    let dir = fixture(
        "exact",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./general/*\"],\"@/one\":[\"./exact/one.ts\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("general/one.ts", "export const y = 2;\n"),
            ("exact/one.ts", "export const z = 3;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/one", &aliases).expect("resolved");
    assert_eq!(found, dir.join("exact").join("one.ts"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rts-host --test resolve_written`
Expected: FAIL — `resolve_written` not found.

- [ ] **Step 3: Write the implementation**

Append to `crates/rts-host/src/graph/resolve.rs`:

```rust
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
pub(super) fn names_the_host(specifier: &str) -> bool {
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
pub(crate) fn resolve_written(
    from: &Path,
    specifier: &str,
    aliases: &super::Aliases,
) -> Option<PathBuf> {
    if is_relative(specifier) {
        return Some(resolve(from, specifier));
    }
    if names_the_host(specifier) {
        return None;
    }
    let base = from.parent()?;
    let _ = base;
    // Every candidate the map offers, in the map's order, and the first that
    // is a real file. `extended` is what "a real file" means here — extension
    // and `index.*` — and it is called rather than reproduced.
    for candidate in aliases.candidates(specifier) {
        let parent = candidate.parent()?;
        let name = candidate.file_name()?.to_str()?;
        if let Some(found) = extended(parent, name) {
            return Some(plain(found));
        }
    }
    None
}
```

Make the two new names reachable from the test. In `crates/rts-host/src/graph/mod.rs`, beside the existing `pub(crate) use resolve::resolve_specifier;`:

```rust
pub use resolve::{names_the_host, resolve_written};
```

and raise `names_the_host`/`resolve_written`'s visibility in `resolve.rs` from `pub(super)`/`pub(crate)` to `pub` if the compiler asks — the test is an integration test and reaches only public items.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rts-host --test resolve_written`
Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rts-host/src/graph/resolve.rs crates/rts-host/src/graph/mod.rs \
        crates/rts-host/tests/resolve_written.rs
git commit -m "feat(host): uma pergunta so — isto nomeia um arquivo, e qual"
```

---

### Task 4: the loader asks the new question

The six sites. Until this task the map exists and nothing consults it.

**Files:**
- Modify: `crates/rts-host/src/graph/mod.rs:116`, `:143`, `:290`, `:299`, `:304`, and the `load` entry
- Modify: `crates/rts-host/src/live.rs:95` (the runtime hook)
- Test: `crates/rts-host/tests/import_alias.rs`

**Interfaces:**
- Consumes: `resolve_written` (Task 3), `Aliases::discover` (Task 2).
- Produces: `graph::tsconfig::active() -> Aliases` reading a `thread_local!`, and `graph::tsconfig::install(Aliases)` called once by `load`.

**Why a `thread_local!` and not a `OnceLock`:** `rts_core::entry::Resolver` is `fn(&str, &str) -> Option<String>` — a bare function pointer with nowhere to put a map — so the map must be reachable without being passed. A `OnceLock` is process-wide, and Rust's test harness runs tests in threads of ONE process: two tests with different configs would read each other's map, and the first to run would win. A `thread_local!` gives each test its own. The risk this leaves is real and Step 6 is the test that catches it: if a program's dynamic `import()` runs on a different thread from the load, the map is empty there.

- [ ] **Step 1: Write the failing test**

```rust
//! An alias, exercised the way a program reaches one: by running.
//!
//! Rule 5 of this crate — a test here runs the program. The assertions are
//! made from INSIDE, through `rts:test`, because a module answers nothing to
//! its host.

use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_alias_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Runs the entry and answers the failures its `rts:test` calls recorded.
fn run(entry: &std::path::Path) -> (usize, Vec<String>) {
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    (reported.len(), failed)
}

#[test]
fn a_static_alias_import_runs_and_answers() {
    let dir = fixture(
        "static",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { add } from \"@/lib/adder\";\n\
                 test(\"aliased\", () => expect(add(2, 3)).toBe(5));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1, "the fixture registers one test");
    assert!(failed.is_empty(), "the alias resolved and the module ran: {failed:?}");
}

/// The no-config guarantee, as a running program: the same shape with no
/// `tsconfig.json` still resolves its relative imports and still runs.
#[test]
fn a_program_with_no_tsconfig_is_unaffected() {
    let dir = fixture(
        "noconfig",
        &[
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { add } from \"./lib/adder\";\n\
                 test(\"relative\", () => expect(add(2, 3)).toBe(5));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(failed.is_empty(), "no config must change nothing: {failed:?}");
}

/// Spec §4 row 2, proven by running: the host module wins over a file that
/// `baseUrl` would otherwise find.
#[test]
fn a_host_module_still_wins_with_base_url_set() {
    let dir = fixture(
        "hostwins",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            // A file that `baseUrl` would resolve `rts:test` to, if a scheme
            // were ever treated as a path. It exports something WRONG on
            // purpose, so reaching it fails loudly instead of passing.
            ("rts/test.ts", "export const test = 0;\nexport const expect = 0;\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 test(\"the harness, not the file\", () => expect(1).toBe(1));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1, "reaching rts/test.ts instead would register nothing");
    assert!(failed.is_empty(), "{failed:?}");
}

/// The runtime hook, not the loader's walk. This is the test that catches the
/// `thread_local!` risk: a dynamic import resolves through
/// `declare_resolver`, which runs while the PROGRAM runs.
#[test]
fn a_literal_dynamic_import_of_an_alias_resolves_at_run_time() {
    let dir = fixture(
        "dynamic",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/late.ts", "export const value = 7;\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const mod = require(\"@/lib/late\");\n\
                 test(\"late\", () => expect(mod.value).toBe(7));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(failed.is_empty(), "the runtime resolver saw the map: {failed:?}");
}

/// A cycle through an alias is refused by name, as a relative cycle is.
#[test]
fn a_cycle_through_an_alias_is_refused_by_name() {
    let dir = fixture(
        "cycle",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/a.ts", "import { b } from \"@/b\";\nexport const a = b;\n"),
            ("src/b.ts", "import { a } from \"@/a\";\nexport const b = a;\n"),
        ],
    );
    let error = rts_host::compile_graph(&dir.join("src/a.ts")).expect_err("a cycle is refused");
    let text = format!("{error:?}");
    assert!(
        text.to_lowercase().contains("cycle"),
        "the refusal names the cycle rather than failing obscurely: {text}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rts-host --test import_alias`
Expected: FAIL — the alias tests fail to resolve `@/…`; `a_program_with_no_tsconfig_is_unaffected` PASSES already, which is the point.

- [ ] **Step 3: Give the map a home and install it**

Append to `crates/rts-host/src/graph/tsconfig.rs`:

```rust
use std::cell::RefCell;

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
    static ACTIVE: RefCell<Aliases> = RefCell::new(Aliases::none());
}

/// Makes this the map for this thread, replacing any previous one.
pub(crate) fn install(aliases: Aliases) {
    ACTIVE.with(|slot| *slot.borrow_mut() = aliases);
}

/// Asks the current map a question, without cloning it.
pub(crate) fn with_active<T>(ask: impl FnOnce(&Aliases) -> T) -> T {
    ACTIVE.with(|slot| ask(&slot.borrow()))
}
```

- [ ] **Step 4: Point the six sites at the question**

In `crates/rts-host/src/graph/mod.rs`:

At the top of `pub fn load(entry: &Path)`, before the walk begins:

```rust
    // The map for this program, found once. Every resolution below and every
    // dynamic one at run time reads this one answer.
    tsconfig::install(Aliases::discover(entry));
```

**`relative_imports` keeps its signature, and gains a sibling.** Its two callers want different questions, and this is not a case where one answer serves both:

- `graph/mod.rs:240` (`visit`) asks *which files is this program made of* — aliases included.
- `rts-cli/src/url_entry.rs:92` (the `rts run https://…` mirror) asks *which relative specifiers must I fetch next*. It holds a `UrlParts`, not a `&Path`, so it could not pass a referrer even if it wanted one — and it must not resolve aliases at all, which is Task 6.

Adding a `from: &Path` parameter would therefore break the remote caller for a capability it must not have. Instead: **one walk, two predicates.** Rename the existing body to take the predicate, and give it two wrappers. Two walks over one tree is what the existing comment at `:120-135` warns against, so the walk is not duplicated.

Replace the body of `relative_imports` (`:100-148`) with:

```rust
/// The specifiers one module writes that `keep` says name a file, in source
/// order.
///
/// The predicate is a parameter and the walk is not duplicated for the same
/// reason the `Wanted` forms below are a parameter: two walks over one tree
/// are two chances for a node to be visited by one and skipped by the other.
fn specifiers_naming_files(
    source: &str,
    keep: impl Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    let mut scratch = Names::default();
    let parsed = parse_module(source, &mut scratch).map_err(|error| format!("{error:?}"))?;
    let mut found = Vec::new();
    for item in &parsed.body {
        let specifier = match item {
            ModuleItem::Import(import) => import.source.clone(),
            ModuleItem::Export(export) => match &export.kind {
                rts_codegen::syntax::ExportKind::Named { source: Some(from), .. } => from.clone(),
                rts_codegen::syntax::ExportKind::All { source, .. } => source.clone(),
                _ => continue,
            },
            ModuleItem::Stmt(_) => continue,
        };
        if keep(&specifier) {
            found.push(specifier);
        }
    }
    let wanted = rts_codegen::emit::Wanted {
        dynamic_import: true,
        require: Some(scratch.intern("require")),
        dynamic_code: None,
    };
    for specifier in rts_codegen::emit::specifiers(&parsed.body, wanted) {
        if keep(&specifier) {
            found.push(specifier);
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
/// resolve an alias from even if it were wanted.
pub fn relative_imports(source: &str) -> Result<Vec<String>, String> {
    specifiers_naming_files(source, is_relative)
}

/// Which FILES this module is made of, aliases included.
///
/// What the graph walk asks. The answer comes from `resolve_written`, the one
/// question, so nothing here learns a second time what a path is.
pub(crate) fn imported_files(source: &str, from: &Path) -> Result<Vec<String>, String> {
    specifiers_naming_files(source, |specifier| {
        tsconfig::with_active(|aliases| resolve_written(from, specifier, aliases)).is_some()
    })
}
```

At `:240`, `visit` calls the new one:

```rust
    for specifier in imported_files(&source, path)
```

and the `resolve(path, &specifier)` on the line after becomes the same single question, so the walk records what the predicate accepted rather than re-deriving it:

```rust
        let resolved = tsconfig::with_active(|aliases| resolve_written(path, &specifier, aliases))
            .expect("imported_files kept only what resolves");
```

In `rewrite` (the three sites at `:290`, `:299`, `:304`), each `if is_relative(source)` becomes:

```rust
        if tsconfig::with_active(|aliases| resolve_written(from, source, aliases)).is_some() {
```

In `resolve.rs`, `resolve_specifier` — the runtime hook — stops asking `is_relative` and asks the same question:

```rust
pub(crate) fn resolve_specifier(from: &str, specifier: &str) -> Option<String> {
    let from = Path::new(from);
    let found = super::tsconfig::with_active(|aliases| resolve_written(from, specifier, aliases))?;
    Some(found.display().to_string())
}
```

Update `relative_imports`'s callers to pass the referrer. Find them with:

```bash
grep -rn "relative_imports" crates/ --include=*.rs
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rts-host --test import_alias`
Expected: PASS, 5 tests. If `a_literal_dynamic_import_of_an_alias_resolves_at_run_time` fails with the module not found, the load and the run are on different threads — STOP and report it; the fix is a decision about where the map lives, not an adjustment here.

- [ ] **Step 6: Run everything, because this is the task that can break every program**

Run: `cargo test -p rts-host 2>&1 | tail -30`
Expected: PASS. Any pre-existing failure must be identical to what `git stash && cargo test -p rts-host` reports.

- [ ] **Step 7: Commit**

```bash
git add crates/rts-host/src/graph/ crates/rts-host/src/live.rs \
        crates/rts-host/tests/import_alias.rs
git commit -m "feat(host): o loader passa a perguntar se o nome escrito e um arquivo"
```

---

### Task 5: both destinations — rule 4

**A correction to the spec, found while planning.** Spec §10 test 2 said to diff the two destinations in a `cargo test`. That is **not possible in this crate**, and `crates/rts-host/tests/aot_object.rs`'s own header says why: running an object needs a linker and the `rts-runtime` staticlib, which `cargo test` does not build. The end-to-end claim is made by the BLOCKING smoke in `.github/workflows/build-artifacts.yml:116-119`:

```bash
target/release/rts run tests/aot/graph.ts > jit.txt
target/release/rts compile tests/aot/graph.ts smoke_graph
./smoke_graph > aot.txt
diff jit.txt aot.txt
```

So the claim is split where the repository already splits it: the crate test asserts the object CARRIES the aliased module, and the alias rides into the existing gate by going into that fixture. No new CI machinery.

**Files:**
- Modify: `tests/aot/graph.ts:18`
- Create: `tests/aot/tsconfig.json`
- Test: `crates/rts-host/tests/import_alias_aot.rs`

**Interfaces:**
- Consumes: everything from Task 4, plus `rts_host::object::compile_graph_to_object` and the `graph`/`entries_in`/`MODULE_TABLE_SYMBOL` helpers of `aot_object.rs`. Adds no production code — if it needs any, spec §6 was wrong and that is the finding.

- [ ] **Step 1: Read the harness before copying its shape**

Read `crates/rts-host/tests/aot_object.rs` in full — its header states what a test here may and may not claim, and Step 3's test must not overclaim.

- [ ] **Step 2: Write the failing crate test**

```rust
//! An aliased module is IN the object, like any other module of the graph.
//!
//! # What this does not claim
//!
//! That the two destinations answer the same thing. Running an object needs a
//! linker and `rts-runtime`'s staticlib, and `cargo test` builds neither —
//! `aot_object.rs`'s header is the long form. That claim is the blocking
//! smoke's, and `tests/aot/graph.ts` now imports through an alias so the
//! smoke makes it.

use rts_host::object::{MODULE_TABLE_SYMBOL, compile_graph_to_object};
use std::path::PathBuf;

fn graph(named: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("rts-alias-aot").join(named);
    let _ = std::fs::remove_dir_all(&dir);
    let mut entry = PathBuf::new();
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(&path, source).expect("a file to write");
        if name.ends_with("app.ts") {
            entry = path;
        }
    }
    entry
}

/// A static alias is rewritten by `rewrite` before either destination sees it,
/// so the object must carry the aliased module as an ordinary one. Spec §6.
#[test]
fn the_object_carries_a_module_reached_through_an_alias() {
    let entry = graph(
        "static",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { add } from \"@/lib/adder\";\nconsole.log(add(2, 3));\n",
            ),
        ],
    );
    let built = compile_graph_to_object(&entry).expect("an aliased graph compiles to an object");
    // Two modules: the entry and the one it reached through the alias. If the
    // alias had been left unresolved the graph would be one module, and the
    // import would have become an undefined name at link time rather than a
    // failure here.
    let count = entries_in(built.bytes(), MODULE_TABLE_SYMBOL, 2);
    assert_eq!(count, 2, "the aliased module is in the object's module table");
}
```

Copy `entries_in` verbatim from `aot_object.rs:51-75` into this file, or make it `pub(crate)` in a shared test module if the crate already has one — check with `ls crates/rts-host/tests/` for a `common.rs` or `mod.rs` before duplicating. Adjust `built.bytes()` to whatever `ObjectProgram` actually exposes; read its definition first.

- [ ] **Step 3: Run it**

Run: `cargo test -p rts-host --test import_alias_aot`
Expected: PASS without production changes. **If the graph compiles to ONE module**, the alias was not resolved on the object path — stop, and report which of spec §6's three bullets was wrong before writing code.

- [ ] **Step 4: Put the alias into the blocking smoke's fixture**

Create `tests/aot/tsconfig.json`:

```json
{
  "compilerOptions": {
    "paths": {
      "@/*": ["./*"]
    }
  }
}
```

In `tests/aot/graph.ts`, change line 18 from:

```ts
import { LABEL, upto, twice } from "./util";
```

to:

```ts
import { LABEL, upto, twice } from "@/util";
```

and add to that file's header comment list, in its established voice:

```
//   - an aliased import          — `@/util`, resolved from tests/aot/tsconfig.json;
//                                   a static alias is rewritten before either
//                                   destination sees it, and the diff is what
//                                   proves that rather than assuming it
```

Leave the `require("./util")` on line 32 relative on purpose: the two forms of the same file now arrive by two different written names, which is a stronger test of "one compilation" than either alone.

- [ ] **Step 5: Run the smoke locally, exactly as CI does**

```bash
cargo build --release -p rts   # o binario e do pacote RAIZ do workspace; `rts-cli` e biblioteca
target/release/rts run tests/aot/graph.ts > /tmp/jit.txt
target/release/rts compile tests/aot/graph.ts /tmp/smoke_graph
if [ -x /tmp/smoke_graph.exe ]; then /tmp/smoke_graph.exe > /tmp/aot.txt; else /tmp/smoke_graph > /tmp/aot.txt; fi
diff /tmp/jit.txt /tmp/aot.txt && echo "IGUAIS"
```

Expected: `IGUAIS`, and the output unchanged from before this task — same lines, same order. A difference here IS the rule 4 violation.

- [ ] **Step 6: The computed specifier**

Append to `import_alias_aot.rs` a test asserting that a computed `require("@/" + name)` is refused by the AOT resolver **by name** — the message says which specifier — while resolving under JIT. Find the existing refusal's wording first:

```bash
grep -rn "computed\|not in the manifest\|refus" crates/rts-runtime/src/*.rs | head
```

Assert against that wording rather than inventing one, so the test fails if the message stops naming the import.

- [ ] **Step 7: Commit**

```bash
git add crates/rts-host/tests/import_alias_aot.rs tests/aot/graph.ts tests/aot/tsconfig.json
git commit -m "test(host): o alias entra no smoke que ja compara os dois destinos"
```

---

### Task 6: a remote program gets no local map — pinned by a test

Task 4 already makes this true by construction: the mirror calls `relative_imports`, which is blind to aliases by type, not by a flag someone could flip. This task is the test that keeps it true, because "it happens to be safe" and "it is guaranteed safe" look identical until someone unifies the two functions.

**Files:**
- Modify: `crates/rts-cli/src/url_entry.rs:83-94` (the doc comment only)
- Test: `crates/rts-host/tests/import_alias.rs` (append)

- [ ] **Step 1: Write the failing test**

```rust
/// A remote program must not resolve `@/…` against THIS machine's project.
///
/// The mirror (`rts run https://…`, `rts-cli/src/url_entry.rs`) asks
/// `relative_imports`, which is blind to aliases. The fixture installs a map
/// that WOULD match, so a pass means the blindness held rather than that
/// nothing matched.
#[test]
fn the_remote_mirror_does_not_resolve_an_alias() {
    let dir = fixture(
        "remote",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/secret.ts", "export const value = 1;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    // Make the local map ACTIVE, exactly as loading a local program would.
    let entry = dir.join("src/app.ts");
    rts_host::compile_graph(&entry).expect("the local program compiles, installing the map");

    let remote = "import { value } from \"@/secret\";\nexport const y = value;\n";
    let found = rts_host::graph::relative_imports(remote).expect("it parses");
    assert!(
        found.is_empty(),
        "a remote program's `@/secret` must not become this machine's file: {found:?}"
    );

    // And the control: a relative one IS still fetched, so the test above is
    // not passing because the walk answers nothing at all.
    let relative = "import { value } from \"./secret\";\nexport const y = value;\n";
    let found = rts_host::graph::relative_imports(relative).expect("it parses");
    assert_eq!(found, vec!["./secret".to_string()]);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p rts-host --test import_alias`
Expected: PASS, and passing on the first run is the correct outcome here — Task 4 made it true by type. **If it fails**, `relative_imports` became alias-aware, which is the defect this task exists to catch.

- [ ] **Step 3: Say why in the place someone would change it**

In `crates/rts-cli/src/url_entry.rs`, extend the doc comment at `:83-85` so the next person does not "fix" the blindness:

```rust
/// Parse `text` (one fetched module) and collect its RELATIVE import
/// specifiers (`./`, `../`), in source order. Builtins/bare specifiers are the
/// engine's job later, on the mirrored files.
///
/// **Aliases are deliberately not resolved here.** `rts_host::graph`'s
/// alias-aware walk is `imported_files`, and calling it from this function
/// would resolve a REMOTE program's `@/…` against the local machine's
/// `tsconfig.json` — reading local files on a remote program's behalf. The
/// test that pins this is `the_remote_mirror_does_not_resolve_an_alias` in
/// `crates/rts-host/tests/import_alias.rs`.
```

- [ ] **Step 4: Commit**

```bash
git add crates/rts-host/tests/import_alias.rs crates/rts-cli/src/url_entry.rs
git commit -m "test(cli): um programa remoto nao herda o tsconfig desta maquina"
```

---

### Task 7: the documentation

Rule 0 of the repo: never leave a rule the code contradicts. Three documents state things this work changes.

**Files:**
- Create: `docs/engine/import-alias.md`
- Modify: `crates/rts-host/README.md` ("What it does not do yet")
- Modify: `docs/README.md` (the index, if it lists `docs/engine/`)

- [ ] **Step 1: Write `docs/engine/import-alias.md`**

Cover, in the voice of the existing `docs/engine/` files — the decision, then why, then the cost:

- The example, and that `tsconfig.json` is the format so an editor needs no plugin.
- The precedence table of spec §4, with row 2's reason stated as the failure it prevents.
- The no-config guarantee.
- One map per program, named as a deliberate subset of `tsc`, with the two-copies-of-one-module reason.
- `paths` semantics: longest literal prefix, exact first, list order, and `extends` resolving targets against the writing file.
- What it does not do: no `node_modules` resolution, no type checking.
- The cost: one `is_file` probe per bare specifier per load, in projects that set `baseUrl`, at load time only.

- [ ] **Step 2: Correct the README**

`crates/rts-host/README.md`'s "What it does not do yet" describes the loader's surface. Add the alias to what it now does, in that section's established voice — the list is kept rather than deleted precisely so what replaced an entry is the thing to read.

- [ ] **Step 3: Verify no document now contradicts the code**

Run: `grep -rn "is_relative" docs/ crates/*/README.md crates/*/PLAN.md`
Any prose saying only a relative specifier names a file is now wrong. Fix each hit.

- [ ] **Step 4: Commit**

```bash
git add docs/engine/import-alias.md crates/rts-host/README.md docs/README.md
git commit -m "docs(host): o alias de import, e a precedencia que o torna seguro"
```

---

### Task 8: the gate

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace --tests 2>&1 | grep -E "^error|^warning: unused" | head -20`
Expected: the two pre-existing broken targets and no new warning from the files this work touched.

- [ ] **Step 2: Full host suite**

Run: `cargo test -p rts-host 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 3: The ceiling, rule 6**

Run: `wc -l crates/rts-host/src/graph/*.rs crates/rts-host/src/jsonc.rs`
Expected: every file under 500. Split before committing if not.

- [ ] **Step 4: The real proof — a real project**

`rts-game` is the program this began for. In that repository, add a `tsconfig.json` with `"@/*": ["./src/*"]`, change ONE deep import (`src/editor/control/commands/scene.ts`'s `../../../compat/math.ts`) to `@/compat/math.ts`, and run its suite with the newly built binary. Expected: 22 of 23 tests still pass — the same count as before, with `test_model.ts`'s pre-existing `buffer.ptr` failure unchanged.

- [ ] **Step 5: Commit any fix, then report**

Report: which tests ran, the counts, and any spec section the implementation contradicted.
