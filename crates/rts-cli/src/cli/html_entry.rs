//! `.html` as an entry point for `compile` and `run` — "só mandar a página e
//! ele compilar sozinho" (the Marcos request this batch answers). Neither
//! command grows a second front end for it: this module writes the shell
//! PROGRAM a user would otherwise write by hand — the loop
//! `scripts/rts_vs_electron/rts/app.ts` already runs — and hands the
//! generated TypeScript text to the exact same compile/run path an ordinary
//! `.ts` entry takes — `compile::command`, and `new_engine::run_path` over a
//! mirrored file (see [`write_shell`]'s own doc for why not `run_source`)
//! never learn the entry was HTML at all; they see one more string of source
//! text.
//!
//! # `casca()`, and why it exists instead of a call to `loadDocument`
//!
//! The shape this shell wants is `loadDocument(html, url): Document` — parse
//! + resources + scripts + `DOMContentLoaded`/`load` in one call — which
//! another lot in flight is building in `rts-dom`. Until it lands, [`CASCA_FN`]
//! composes the same result from what already exists:
//! `parseDocument`+`loadResources`+`runScriptsAt`, exactly the sequence
//! `app.ts` hand-writes today. Its own leading comment marks the one line to
//! delete once `loadDocument` exists, so swapping it is a one-line change to
//! this module rather than a rewrite of the generator.
//!
//! # Embedded vs read from disk
//!
//! `rts compile` ships a binary that may run anywhere its own resources are
//! not, so its HTML travels as a JSON-escaped literal baked in at build
//! time — never read from disk again ([`for_compile`]). `rts run` is a JIT
//! invocation of a file that is right there, so it reads the SAME way
//! `examples/view.ts` does today ([`for_run`]): editing the page and
//! re-running needs no rebuild.
//!
//! What compiling pays for that running does not: a relative `<link>`/`<img>`
//! resolves against the HTML's own folder as it exists on the machine running
//! `rts compile`, at build time — not a run-time lookup. A copy of the `.exe`
//! moved to a machine without that exact path loses those resources; this is
//! named here, in `--help` and in `docs/engine/aot-page-scripts.md` rather
//! than left for a user to discover silently.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The loop every generated program shares — `app.ts`'s own sequence, wrapped
/// in a function whose `resourceBase`/`scriptUrl`/`title` arguments stand in
/// for the single `loadDocument(html, url)` call this shell will make once
/// that lot lands.
///
/// `resourceBase` and `scriptUrl` stay two parameters rather than one `url`
/// because they answer different questions: `loadResources` resolves a
/// relative HREF against a filesystem/URL base, `runScriptsAt` publishes a
/// `window.location` a script may read — `app.ts` keeps them apart for the
/// same reason, and a page whose scripts read `location.href` while its
/// `<link>`s resolve against a local folder needs both answers to be real.
const CASCA_FN: &str = r#"
// `loadDocumentFrom` (crates/rts-dom/src/lifecycle.ts) e a navegacao da HTML
// spec: parse, recursos, os <script> do parse por ordem, DOMContentLoaded,
// load — e a partir dai um <script> anexado por appendChild corre sozinho. A
// base dos recursos e a pasta do HTML; a URL dos scripts e a de pagina.
function casca(html: string, resourceBase: string, scriptUrl: string, title: string): void {
  const doc = loadDocumentFrom(html, scriptUrl, resourceBase);
  console.log("pagina carregada: " + doc.readyState + ", scripts: " + doc.querySelectorAll("script").length);
  const win = egui.openWindow(title, 1100, 750, 0);
  while (egui.isOpen(win)) {
    if (!egui.pump(win)) break;
    egui.beginFrame(win);
    egui.render(win, doc._dom);
    egui.endFrame(win);
    pumpInputEvents(doc);
    pumpEventCallbacks(doc);
    pumpTimerCallbacks(doc);
  }
  egui.close(win);
}
"#;

/// The script scope URL every generated page gets. Nothing downstream
/// compares it to anything real — it only has to look like one, for
/// `window.location` and a UMD bundle's origin sniff — so a fixed literal is
/// enough. `app.ts` uses the same one.
const SCRIPT_URL: &str = "https://localhost/";

/// `true` for a `.html`/`.htm` path, matched case-insensitively — a page saved
/// from Windows Explorer or fetched from a server may carry either case.
pub fn is_html(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("html") | Some("htm")
    )
}

/// The page's own `<title>`, or the file's stem when the page has none — so a
/// window never opens with a blank bar. See
/// [`rts_host::object::html_scripts::title`]'s own doc for how it is measured.
fn window_title(html: &str, fallback: &Path) -> String {
    rts_host::object::html_scripts::title(html).unwrap_or_else(|| {
        fallback
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "RTS".to_owned())
    })
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("a Rust &str always encodes as a JSON string")
}

/// The AOT shell for `rts compile pagina.html`: the HTML embedded as a
/// JSON-escaped literal (never read from disk at run time), resolving
/// relative resources against `entry`'s own folder as it exists on THIS
/// machine right now — the build-time path this module's own doc names.
///
/// `entry`'s own `<script>`s are NOT compiled by this function — `casca`'s
/// call to `runScriptsAt` reaches them at run time, through whichever
/// archive `rts compile` linked. Precompiling them (`--html`-style, into the
/// hash lookup) is the caller's job: push `entry` onto the same list a literal
/// `--html <file>` would have populated, exactly as if it had been given.
pub fn for_compile(entry: &Path, html: &str) -> Result<String> {
    let title = window_title(html, entry);
    let resource_base = std::path::absolute(entry)?.to_string_lossy().into_owned();
    Ok(format!(
        "import egui from \"rts:egui\";\n{CASCA_FN}\ncasca({}, {}, {}, {});\n",
        json_string(html),
        json_string(&resource_base),
        json_string(SCRIPT_URL),
        json_string(&title),
    ))
}

/// The JIT shell for `rts run pagina.html`: reads the page from disk at run
/// time, exactly as `examples/view.ts` does today — no `--html`
/// precompilation, so editing the page needs no rebuild between runs.
pub fn for_run(html: &str, entry: &Path) -> String {
    let title = window_title(html, entry);
    let path = json_string(&entry.to_string_lossy());
    format!(
        "import egui from \"rts:egui\";\nimport {{ readFileSync }} from \"node:fs\";\n\
         const __rtsPagePath: string = {path};\n{CASCA_FN}\n\
         casca(readFileSync(__rtsPagePath, \"utf8\") as string, __rtsPagePath, {url}, {title});\n",
        url = json_string(SCRIPT_URL),
        title = json_string(&title),
    )
}

/// Mirrors a generated program into the system temp dir and answers its
/// LOCAL path — for [`super::new_engine::run_path`], which runs on the
/// CALLING thread rather than a spawned one, exactly the property a window
/// needs (see `run.rs`'s own comment on why this is not `run_source`).
/// Named after `entry`'s own stem so a failing run leaves something a person
/// can open and re-run by hand, the same reasoning `aot_object.rs`'s test
/// helper states for its own temp files.
pub fn write_shell(source: &str, entry: &Path) -> Result<PathBuf> {
    let stem = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("pagina");
    let dir = std::env::temp_dir().join("rts-html-run");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(format!("{stem}.rts-shell.ts"));
    std::fs::write(&path, source).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}
