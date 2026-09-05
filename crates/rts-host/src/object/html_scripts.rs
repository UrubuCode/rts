//! What `rts compile --html` reads before it compiles anything: the exact
//! `<script>` sources a page carries, and the static surface a `window`
//! object always has, regardless of which page loads into it.
//!
//! # Why the extraction rule lives here and not in `.ts`
//!
//! `crates/rts-dom/src/dom.ts`'s `__runScriptAt` is the RULE — which `type`
//! values are JavaScript, how a `src="data:…"` decodes — and this module
//! reads the same tree that function reads, through the same Rust it calls
//! through: [`rts_dom::parse_html_to_dom`] and [`rts_dom::dom::Dom::query_all`]
//! are the exact functions behind the `parseHtml`/`getByTagCount`/`getByTagAt`
//! natives `__runScriptAt` calls, so the ORDER and the SET of `<script>` nodes
//! this walk finds cannot drift from what a JIT run finds — there is only one
//! HTML tree-builder in this engine, and both callers ask it the same
//! question.
//!
//! What is NOT reused, and is restated here in ~20 lines: the `type`
//! allow-list and the `data:` decode. `__runScriptAt` is TypeScript compiled
//! BY this engine, and this module runs BEFORE any TypeScript compiles at
//! all — reusing it would mean running the JIT engine over the prelude just to
//! reach two small, stable checks, which is a heavier and stranger dependency
//! than restating them with a comment pointing back at the source of truth.
//! If that allow-list ever grows a case, both places need it; the fixture in
//! `tests/aot/claude-pagina-com-script.html` and its `.test.ts` sibling name
//! the behaviour rather than the function, so a future drift fails a real
//! comparison rather than passing two unrelated tests.
//!
//! # Why `window`'s surface is measured rather than listed
//!
//! A page `<script>`'s free names — `document`, `location`, `addEventListener`
//! — resolve through the `window` object `rts-dom-bridge`'s `DomScope.run`
//! hands the compiler at JIT time, and the compiler learns what is on it by
//! calling [`rts_core::entry::environment_names`] on the LIVE object. There is
//! no live object yet when `rts compile` runs — nothing has executed — so
//! [`window_base`] builds one exactly the way a page does
//! (`crates/rts-dom/src/window.ts`'s `__winFor`) and asks the same question of
//! it, through one throwaway in-memory run of this engine's own JIT compiler.
//! That is more machinery than a hand-written list of names, and it is the
//! only version of the list that cannot go stale the day `WindowImpl` gains or
//! loses a member: a hand-written list is exactly the second statement of one
//! fact this repository's own rules exist to refuse.

use crate::link::HostError;
use crate::run::Scoped;

/// One page's `<script>` sources, in document order — inline text, or a
/// `data:` URI decoded, exactly as [`extract`]'s own header states the rule.
pub fn extract(html: &str) -> Vec<String> {
    let dom = rts_dom::parse_html_to_dom(html);
    let mut sources = Vec::new();
    for id in dom.query_all("script") {
        let kind = dom.get_attr(id, "type").unwrap_or_default().to_lowercase();
        // The allow-list `__runScriptAt` states: an empty `type`, the four
        // JavaScript MIME spellings a page might write, or `"module"` — which
        // this engine, like that function, runs as an ordinary script rather
        // than as an ES module. `application/json` and every other data
        // island a page carries falls through and is skipped.
        let is_js = kind.is_empty()
            || kind == "text/javascript"
            || kind == "application/javascript"
            || kind == "module"
            || kind == "application/x-javascript"
            || kind == "text/ecmascript";
        if !is_js {
            continue;
        }
        let mut code = dom.text_content(id).unwrap_or_default();
        let src = dom.get_attr(id, "src").unwrap_or_default();
        // `src="http…"` is deliberately left alone: an external script is
        // fetched by whatever loaded the page (a browser, or this engine's own
        // `loadResources`), never by this compiler, which touches no network
        // and no disk beyond the `--html` files it was handed.
        if src.len() > 5 && src.as_bytes()[..5].eq_ignore_ascii_case(b"data:") {
            let Some(comma) = src.find(',') else {
                continue;
            };
            let meta = &src[..comma];
            let payload = &src[comma + 1..];
            code = if meta.contains("base64") {
                // The same bytes `atob` would answer, read one code unit per
                // byte — see `rts_core::entry::decode_base64`'s own doc for
                // why base64 is decoded once, from one function, on both the
                // JIT and this build-time path.
                rts_core::entry::decode_base64(payload)
                    .into_iter()
                    .map(char::from)
                    .collect()
            } else {
                percent_decode(payload)
            };
        }
        if code.is_empty() {
            continue;
        }
        sources.push(code);
    }
    sources
}

/// A permissive stand-in for `decodeURIComponent`, for the one `data:` shape
/// `atob` does not cover: a percent-encoded (non-base64) payload.
///
/// Named DIFFERENT from the language's own function on purpose. The real
/// `decodeURIComponent` — `rts_core::entry::uri`'s `decoded` — walks UTF-16
/// code units through an active `Context` and raises `URIError` on a malformed
/// escape or a lone surrogate; neither exists yet when `rts compile` runs this.
/// This walks UTF-8 bytes instead and answers whatever it can decode, silently
/// keeping bytes it cannot — which is the WHATWG-adjacent leniency `unescape`
/// already has in this engine, applied here because a build tool refusing to
/// compile over one malformed percent-escape in a rarely-used branch (real
/// pages overwhelmingly ship `data:` scripts as base64, per `dom.ts`'s own
/// header) is a worse failure than decoding it approximately.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok());
            if let Some(byte) = hex {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads and extracts every `<script>` `rts compile --html` was pointed at,
/// one file after another, in the order they were given.
pub fn extract_files(paths: &[std::path::PathBuf]) -> Result<Vec<String>, HostError> {
    let mut sources = Vec::new();
    for path in paths {
        let html = std::fs::read_to_string(path)
            .map_err(|error| HostError::Malformed(format!("{}: {error}", path.display())))?;
        sources.extend(extract(&html));
    }
    Ok(sources)
}

/// The property names a page `<script>`'s free identifiers may resolve
/// through — every own and inherited key of a `window` this engine built for
/// itself, measured rather than listed. See this module's own header.
pub fn window_base() -> Result<Vec<String>, HostError> {
    // `__winFor` needs a document handle; an empty one is enough; nothing
    // about `WindowImpl`'s OWN surface depends on what a page put in it. The
    // bootstrap answers a NEWLINE-joined list rather than the window object
    // itself: a JIT run's context — and the heap the window lives in — does
    // not survive past `Compiled::run`, but the STRING this reads back does,
    // copied out while that heap still existed (`Compiled::described`'s own
    // doc explains why that is the one thing a caller CAN still ask for).
    const BOOTSTRAP: &str = r#"
        const __doc = parseDocument("<html></html>");
        const __w: any = __winFor(__doc._dom, "https://localhost/", 1000, 800);
        const __seen: string[] = [];
        let __cur: any = __w;
        let __hops = 0;
        while (__cur !== null && __cur !== undefined && __hops < 16) {
            const __own = Object.getOwnPropertyNames(__cur);
            for (const __k of __own) {
                if (__seen.indexOf(__k) < 0) __seen.push(__k);
            }
            __cur = Object.getPrototypeOf(__cur);
            __hops = __hops + 1;
        }
        return __seen.join("\n");
    "#;
    // NOT `crate::run::compile` — that goes through `front_end`, which prepends
    // the DOM facade and then asks "does this look like a module?" by
    // substring. `crates/rts-dom/src/dom.ts` has the word "import" followed by
    // a space in a comment ("Cada import é resolvido…"), so EVERY program that
    // uses the facade is parsed as a module today — harmless for one with no
    // top-level `return`, fatal for this bootstrap: a module's body is never
    // wrapped in a function, so SWC's TypeScript dialect (which, unlike its
    // ECMAScript one, carries no `allow_return_outside_function` escape hatch)
    // refuses the `return` above with "Return statement is not allowed here".
    //
    // `Scoped::Eval` sidesteps it rather than fixing the substring test (out of
    // scope here, and a real fix wants a parse of the TEXT rather than a
    // grep of it): `front_end_agreeing`'s module guess only fires for
    // `Scoped::Nothing`, so asking for `Eval` scoping — with nothing actually
    // enclosing this program — takes the SCRIPT door unconditionally, which is
    // the one that both wraps in a function (return legal) and turns a
    // trailing expression into the completion value `Compiled::described`
    // reads back.
    let source_with_dom = crate::run::with_dom_facade(BOOTSTRAP);
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
    compiled.run();
    let names = compiled.described().ok_or_else(|| {
        HostError::Malformed(
            "the window-surface bootstrap did not answer text — this is a defect in \
             `rts-host::object::html_scripts`, not in a compiled program"
                .to_owned(),
        )
    })?;
    Ok(names
        .lines()
        .filter(|name| !name.is_empty() && !name.starts_with("__rts_"))
        .map(str::to_owned)
        .collect())
}
