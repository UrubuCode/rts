//! What `rts compile` embeds for a page besides its scripts: every local file
//! the page's own loader reads — the `<link>` sheets, what they `@import`,
//! the `<script src>` files and the local `<img>`s (lot AOT-1).
//!
//! # Recorded by running the loader, not by re-deciding it
//!
//! Which files a page references is decided by ONE piece of code:
//! `loadResources` in `crates/rts-dom/src/dom.ts`, the loader every JIT run
//! and every compiled binary already runs at start-up. [`record`] runs that
//! same function once, in a throwaway JIT — the pattern
//! [`super::html_scripts::window_base`] uses, for the same reason — with
//! `rts_dom_bridge::recursos::tabela` RECORDING what the two reading natives
//! fetched from disk. The alternative that lost was walking the parsed tree
//! here for `<link>`/`<img>`/`<script src>` and scanning CSS for `@import`: a
//! second resolver, correct until the day the loader learns a case it does not.
//!
//! Only `parseDocument` + `loadResources` run — not `loadDocumentFrom`, which
//! would also run the page's `<script>`s: executing arbitrary page code on
//! the build machine to learn which files it reads is neither needed (script
//! execution reads nothing through these natives) nor safe.
//!
//! # The key is the base
//!
//! The table is keyed by the exact path string the loader resolves, which is
//! a function of the resource base it was given. So the base is computed in
//! ONE place, [`resource_base`], which `rts compile pagina.html`'s shell and
//! this recorder both call — and it is spelled the way `__dirname` is spelled
//! (canonical, Windows' verbatim prefix stripped: `crate::graph::settled`),
//! so a `.ts` driver passing `__dirname + "/page.html"` produces the same keys.

use std::path::{Path, PathBuf};

use crate::link::HostError;
use crate::run::Scoped;

/// One embedded resource: the path the loader asked for, and its bytes.
pub type Resource = (String, Vec<u8>);

/// The resource base of a page on disk: the path its relative `href`/`src`
/// resolve against, at build time AND in the compiled binary. See this
/// module's header for why there is exactly one function answering it.
pub fn resource_base(page: &Path) -> String {
    crate::graph::settled(page.to_path_buf()).display().to_string()
}

/// Every resource of every page `rts compile --html` was given, recorded by
/// running each page's loader. A path two pages share is kept once.
pub fn record_files(paths: &[PathBuf]) -> Result<Vec<Resource>, HostError> {
    let mut all: Vec<Resource> = Vec::new();
    for path in paths {
        let html = std::fs::read_to_string(path)
            .map_err(|error| HostError::Malformed(format!("{}: {error}", path.display())))?;
        for (key, bytes) in record(&html, &resource_base(path))? {
            if !all.iter().any(|(seen, _)| *seen == key) {
                all.push((key, bytes));
            }
        }
    }
    Ok(all)
}

/// The resources one page's loader reads from disk, given its HTML and base.
pub fn record(html: &str, base: &str) -> Result<Vec<Resource>, HostError> {
    let literal = |text: &str| {
        serde_json::to_string(text).expect("a Rust &str always encodes as a JSON string")
    };
    // `return 0` so the run answers something `described` can read and the
    // Eval door below accepts; the answer itself is not used — what matters is
    // what the natives recorded while it ran.
    let bootstrap = format!(
        "const __doc = parseDocument({});\nloadResources(__doc, {});\nreturn 0;\n",
        literal(html),
        literal(base),
    );
    // The same door `window_base` takes, and for the reason its own comment
    // gives at length: `crate::run::compile` would guess "module" from the
    // DOM facade's text and refuse the top-level `return`.
    let source_with_dom = crate::run::with_dom_facade(&bootstrap);
    let front = crate::run::front_end_agreeing(
        &source_with_dom,
        None,
        false,
        Scoped::Eval { enclosing: &[], hide_node_globals: false },
    )?;
    let mut compiled = crate::run::assemble(
        front.emitted,
        &[],
        1,
        front.model,
        front.funcs,
        front.types,
        front.calls,
        front.names,
        Vec::new(),
    )?;
    // Recording is thread-local, and `Compiled::run` runs the program on THIS
    // thread (`run_region` installs its context around the call), which is
    // what makes turning it on here see every read the loader makes.
    rts_dom_bridge::recursos::tabela::record_into();
    compiled.run();
    Ok(rts_dom_bridge::recursos::tabela::take_recorded())
}

#[cfg(test)]
mod tests {
    use super::{record, resource_base};

    /// The claim AOT-1 rests on: running the page's own loader finds every
    /// local file it references — a linked sheet, the sheet that one
    /// `@import`s, a `<script src>` and an `<img>` — keyed by the path the
    /// loader resolved from the base, and nothing it does not read.
    #[test]
    fn the_loader_run_records_every_local_resource_of_a_page() {
        let dir = std::env::temp_dir().join("rts-host-page-resources").join("page");
        std::fs::create_dir_all(dir.join("r")).expect("a scratch directory");
        std::fs::write(dir.join("r/a.css"), "@import \"b.css\";\n#x { width: 3px; }\n")
            .expect("a sheet");
        std::fs::write(dir.join("r/b.css"), "#x { height: 4px; }\n").expect("an import");
        std::fs::write(dir.join("r/s.js"), "var ran = 1;\n").expect("a script");
        std::fs::write(dir.join("r/i.png"), [0x89u8, b'P', b'N', b'G']).expect("an image");
        let page = dir.join("page.html");
        std::fs::write(&page, "").expect("the page itself");
        let html = "<link rel=\"stylesheet\" href=\"r/a.css\"><div id=x></div>\
                    <img src=\"r/i.png\"><img src=\"r/absent.png\">\
                    <script src=\"r/s.js\"></script>";

        let base = resource_base(&page);
        let recorded = record(html, &base).expect("the loader runs");
        let names: Vec<&str> = recorded
            .iter()
            .map(|(path, _)| path.rsplit(['/', '\\']).next().unwrap_or(path))
            .collect();
        assert_eq!(names, ["a.css", "b.css", "s.js", "i.png"]);
        let dir_of_base = &base[..base.rfind(['/', '\\']).expect("a base with a folder")];
        assert!(
            recorded.iter().all(|(path, _)| path.starts_with(dir_of_base)),
            "every key is resolved from the base: {recorded:?}"
        );
        assert_eq!(recorded[1].1, b"#x { height: 4px; }\n");
    }
}
