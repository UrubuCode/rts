//! Run a program straight from an http(s) URL — `rts run https://…/main.ts`.
//!
//! The URL entry is fetched together with its whole RELATIVE import graph
//! (`./x`, `../y`), mirrored into a per-run folder under the SYSTEM temp dir,
//! and the LOCAL entry path is handed to the normal disk pipeline
//! (`run_path`). Nothing is stored in the register on purpose: a URL's content
//! can change at any time, and a persistent cache would keep serving stale
//! code — the mirror is re-downloaded on every run.
//!
//! HTTP goes through the SAME stack as the global `fetch()` (ureq), and every
//! request identifies with the same browser-style User-Agent
//! (`Rts v<version>` / `Rts development` — `rts_runtime::fetch_user_agent`).
//!
//! Relative specifiers resolve with the SAME candidate list as the disk
//! resolver (`rts-codegen-new/front/modules/resolve.rs`): explicit extension
//! as-is; otherwise `x.ts, x.rts, x.js, x/index.ts, x/index.rts, x/index.js`
//! probed in order (first HTTP 200 wins). Builtins (`rts:*`, `node:*`) and
//! bare npm specifiers are left untouched for the engine to handle.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use rts_ast::ast::Item;

/// `true` when the CLI input names an http(s) URL instead of a local file.
pub fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

/// Candidate lists mirrored from the disk resolver (same precedence order).
const CANDIDATE_EXTS: [&str; 3] = ["ts", "rts", "js"];
const INDEX_FILES: [&str; 3] = ["index.ts", "index.rts", "index.js"];

/// Download `entry_url` and every reachable relative import into a fresh
/// temp-dir mirror; return the LOCAL entry path for the disk pipeline.
pub fn fetch_program(entry_url: &str) -> Result<PathBuf> {
    let entry = UrlParts::parse(entry_url)?;
    let root = mirror_root()?;

    let entry_text = fetch_text(&entry.to_url())?
        .ok_or_else(|| anyhow!("HTTP 404 fetching entry {}", entry.to_url()))?;

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(UrlParts, String)> = VecDeque::new();
    visited.insert(entry.to_url());
    queue.push_back((entry.clone(), entry_text));

    while let Some((url, text)) = queue.pop_front() {
        let local = url.local_path(&root);
        if let Some(dir) = local.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating mirror dir {}", dir.display()))?;
        }
        std::fs::write(&local, &text)
            .with_context(|| format!("writing mirror file {}", local.display()))?;

        for spec in relative_imports(&text, &url)? {
            let (dep_url, dep_text) = resolve_remote(&url, &spec)?;
            if visited.insert(dep_url.to_url()) {
                queue.push_back((dep_url, dep_text));
            }
        }
    }

    Ok(entry.local_path(&root))
}

/// Fresh per-run mirror root under the system temp dir. Recreated on every run
/// so a re-run always sees the CURRENT remote content (no stale cache).
fn mirror_root() -> Result<PathBuf> {
    let root = std::env::temp_dir().join("rts-url-run");
    if root.exists() {
        // Best-effort clean of the previous run's mirror; a locked file (e.g.
        // an editor holding it open) is not fatal — files are overwritten.
        let _ = std::fs::remove_dir_all(&root);
    }
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating mirror root {}", root.display()))?;
    Ok(root)
}

/// Parse `text` (one fetched module) and collect its RELATIVE import
/// specifiers (`./`, `../`), in source order. Builtins/bare specifiers are the
/// engine's job later, on the mirrored files.
fn relative_imports(text: &str, url: &UrlParts) -> Result<Vec<String>> {
    let program = rts_parser::parse_source(text)
        .map_err(|e| anyhow!("parse of {} failed: {e}", url.to_url()))?;
    let mut specs = Vec::new();
    for item in &program.items {
        let from = match item {
            Item::Import(decl) => &decl.from,
            Item::ExportNamespace(d) => &d.from,
            _ => continue,
        };
        if from.starts_with("./") || from.starts_with("../") {
            specs.push(from.clone());
        }
    }
    Ok(specs)
}

/// Resolve a relative `spec` against the importing module's URL, probing the
/// same candidate list as the disk resolver. Returns the winning URL + its
/// body (the probe already downloaded it — no double fetch).
fn resolve_remote(from: &UrlParts, spec: &str) -> Result<(UrlParts, String)> {
    let base = from.join(spec)?;
    let has_ext = base
        .segments
        .last()
        .is_some_and(|s| s.rsplit_once('.').is_some_and(|(stem, _)| !stem.is_empty()));

    let mut candidates: Vec<UrlParts> = Vec::new();
    if has_ext {
        candidates.push(base);
    } else {
        for ext in CANDIDATE_EXTS {
            let mut c = base.clone();
            if let Some(last) = c.segments.last_mut() {
                *last = format!("{last}.{ext}");
            }
            candidates.push(c);
        }
        for idx in INDEX_FILES {
            let mut c = base.clone();
            c.segments.push(idx.to_string());
            candidates.push(c);
        }
    }

    let mut tried = Vec::new();
    for cand in candidates {
        let url = cand.to_url();
        if let Some(text) = fetch_text(&url)? {
            return Ok((cand, text));
        }
        tried.push(url);
    }
    bail!(
        "module not found: '{spec}' (resolved from {}); tried:\n  {}",
        from.to_url(),
        tried.join("\n  ")
    )
}

/// GET `url` with the shared RTS User-Agent. `Ok(None)` on 404 (candidate
/// probing), error on any other failure.
fn fetch_text(url: &str) -> Result<Option<String>> {
    let resp = ureq::get(url)
        .set("User-Agent", rts_runtime::fetch_user_agent())
        .call();
    match resp {
        Ok(r) => Ok(Some(
            r.into_string().with_context(|| format!("reading body of {url}"))?,
        )),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(ureq::Error::Status(code, _)) => bail!("HTTP {code} fetching {url}"),
        Err(e) => bail!("network error fetching {url}: {e}"),
    }
}

/// A decomposed http(s) URL: scheme + host (with optional port) + normalized
/// path segments. Query/fragment are dropped — they can't name a file in the
/// mirror and the raw-source hosts we target don't need them.
#[derive(Debug, Clone)]
struct UrlParts {
    scheme: String,
    host: String,
    segments: Vec<String>,
}

impl UrlParts {
    fn parse(url: &str) -> Result<Self> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| anyhow!("not a URL: {url}"))?;
        if scheme != "http" && scheme != "https" {
            bail!("unsupported URL scheme '{scheme}' in {url}");
        }
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if host.is_empty() {
            bail!("URL has no host: {url}");
        }
        let segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if segments.is_empty() {
            bail!("URL has no file path: {url}");
        }
        Ok(Self {
            scheme: scheme.to_string(),
            host: host.to_string(),
            segments,
        })
    }

    fn to_url(&self) -> String {
        format!("{}://{}/{}", self.scheme, self.host, self.segments.join("/"))
    }

    /// Resolve a relative `spec` against THIS module's URL (drop the filename,
    /// then walk `.`/`..` segments). `..` above the host root is an error.
    fn join(&self, spec: &str) -> Result<Self> {
        let mut segments = self.segments.clone();
        segments.pop(); // the importing module's own filename
        for part in spec.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if segments.pop().is_none() {
                        bail!(
                            "relative import '{spec}' escapes above the host root \
                             (resolved from {})",
                            self.to_url()
                        );
                    }
                }
                seg => segments.push(seg.to_string()),
            }
        }
        Ok(Self {
            scheme: self.scheme.clone(),
            host: self.host.clone(),
            segments,
        })
    }

    /// The mirror path of this URL: `<root>/<host>/<segments...>`, with the
    /// characters Windows forbids in file names replaced by `_`.
    fn local_path(&self, root: &std::path::Path) -> PathBuf {
        let mut path = root.join(sanitize(&self.host));
        for seg in &self.segments {
            path.push(sanitize(seg));
        }
        path
    }
}

/// Replace filesystem-hostile characters (`:` from a port, `%`-escapes kept
/// as-is are fine, but `*?"<>|` are not) so a URL segment is a valid file name
/// on every platform.
fn sanitize(segment: &str) -> String {
    segment
        .chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' => '_',
            c => c,
        })
        .collect()
}
