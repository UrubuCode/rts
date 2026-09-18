//! `rts:json5` — JSON5, the superset of `JSON` with comments, trailing
//! commas, unquoted keys, single-quoted strings and hex literals.
//!
//! # Why a MODULE, and not the bare `JSON5` global the old engine had
//!
//! `crates/rts-shared/src/globals/json5/mod.rs` (deleted 2026-08-10, with the
//! rest of the old engine) registered `JSON5` as a global — reachable with no
//! `import`, the same as `JSON`. **Neither Node nor Bun has a global `JSON5`
//! at all**; it is not a JavaScript surface, it is a library some programs
//! `import` from npm. Checked directly (`node -e "typeof JSON5"` and the
//! equivalent under `bun`) rather than assumed. `docs/…`'s membership rule for
//! a bare global is availability *and* being part of the language every
//! target already agrees on — `console`, `TextEncoder`, `fetch` all clear that
//! bar because a program written against a real runtime already expects them
//! with no import line. `JSON5` clears neither: no real runtime provides it
//! unprompted, so reviving it as a global would be this engine inventing a
//! surface no other JavaScript has, which is exactly what `BRIEF-SUITE.md`'s
//! rule (and CLAUDE.md's "the régua de verdade é o Node") refuses.
//!
//! What is real about it: it is *this engine's own* convenience, same
//! footing as `rts:test` or the bare `rts` specifier — worth keeping because
//! `tests/edge_json5.test.ts` already exercises a real use (config-file-shaped
//! JSON with comments), and because the parser and the crate it needs
//! (`json5`, already vetted-adjacent: `docs/reference/node/crates.md`
//! already accepts pure-Rust Serde-speaking parsers of this shape for other
//! surfaces) cost nothing to keep behind an explicit `import { JSON5 } from
//! "rts:json5"` instead of a bare name. `tests/edge_json5.test.ts` was edited
//! to add that import line — the one exception `BRIEF-SUITE.md` names to
//! "never edit a test to make it pass": the test's ORIGINAL shape (a bare
//! global) asserted something that contradicts Node and Bun both, so
//! correcting it to the shape this engine's own surface actually has is the
//! deliverable, not a test-shopping edit.
//!
//! # Why `parse` delegates to the real `JSON.parse` rather than building the
//! heap tree itself
//!
//! `rts-core::entry::json::read` already turns text into a rooted heap value
//! correctly — shape-batched objects, `Rooted` guards across every allocation,
//! the reviver hook. A second walk here, from `json5`'s own parsed tree onto
//! the heap, would be exactly the "two tables of one number" `reuse-check`
//! (§3) warns about, except the "number" is a whole materialisation algorithm.
//! So `parse` asks `json5` to re-lex the lenient syntax down to
//! `serde_json::Value` (a Rust-only tree, no heap involved), serialises THAT
//! back out as strict JSON text, and hands it to the global `JSON.parse` the
//! same way a program spelling `JSON.parse(text)` would reach it — the same
//! "ask the machine, don't re-derive it" this crate's `console::inspect`
//! already does for `%j` (`inspect::json_stringify`, which calls `JSON.stringify`
//! through the global rather than writing a second serialiser).
//!
//! One divergence from that: `serde_json::Value` cannot hold `NaN`/`Infinity`
//! (`serde_json::Number` refuses a non-finite `f64`), so a JSON5 document using
//! either — legal JSON5, illegal strict JSON — fails to parse here. Named
//! rather than silently answering the wrong number; no fixture in this
//! repository exercises it.
//!
//! # `stringify`
//!
//! Reuses `JSON.stringify` outright (matching the old engine's own doc:
//! "Stringify reusa JSON.stringify") — JSON5's grammar is a strict superset
//! for READING, and every value this engine can hold already has a valid
//! strict-JSON spelling to write, so there is nothing JSON5-specific to do on
//! the way out.
//!
//! # Error handling
//!
//! `parse` of invalid JSON5 answers the NUMBER `0`, matching the old global's
//! own contract (`AbiType::U64` return, "0 on error" in its doc) rather than
//! `JSON.parse`'s `SyntaxError` throw — a decision this port keeps rather than
//! silently changes, since `tests/edge_json5.test.ts` pins it.

use rts_core::entry::{self, Context, Provided};

const MEMBERS: &[(&str, Provided)] = &[("parse", parse), ("stringify", stringify)];

/// The namespace `rts:json5` is: one named export, `JSON5` — an object (no
/// constructor — `JSON5` is not a class in the old surface either) with
/// `parse` and `stringify` — so `import { JSON5 } from "rts:json5"` finds it
/// the same way `import { Console } from "node:console"` finds a member of
/// ITS namespace object rather than being the namespace's own methods.
pub fn namespace(context: &mut Context) -> u64 {
    let json5 = entry::make_namespace(context, MEMBERS);
    let module = entry::make_namespace(context, &[]);
    entry::put_member(context, module, "JSON5", json5);
    module
}

/// `JSON5.parse(text)`. `reviver` is not read — the old engine's own
/// signature (`Sig::new(vec![AbiType::StrPtr], AbiType::U64)`) never carried
/// one either.
extern "C" fn parse(_e: u64, _this: u64, text: u64, _reviver: u64, _c: u64, _d: u64) -> u64 {
    let Some(source) = entry::text_of(text).or_else(|| entry::described(text)) else {
        return entry::make_number(0.0);
    };
    let Ok(value) = json5::from_str::<serde_json::Value>(&source) else {
        return entry::make_number(0.0);
    };
    let Ok(strict_json) = serde_json::to_string(&value) else {
        return entry::make_number(0.0);
    };
    let json_ns = entry::global_get(well_known("JSON"));
    let parse_fn = entry::get_property(json_ns, well_known("parse"));
    let text = entry::with_runtime(|context| entry::make_string(context, &strict_json));
    let undefined = entry::undefined_value();
    entry::call(parse_fn, undefined, text, undefined, undefined, undefined)
}

/// `JSON5.stringify(value)` — `JSON.stringify`, unchanged. See the module
/// doc's "stringify" section for why there is nothing JSON5-specific here.
extern "C" fn stringify(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let json_ns = entry::global_get(well_known("JSON"));
    let stringify_fn = entry::get_property(json_ns, well_known("stringify"));
    let undefined = entry::undefined_value();
    entry::call(stringify_fn, undefined, value, undefined, undefined, undefined)
}

/// The property-key number for a name reached by lookup rather than by a
/// compiled site — `console::inspect`'s own `well_known` does the identical
/// two steps for the identical reason (intern, then `ToPropertyKey`); a
/// third copy inside `rts-core` would be the thing to reuse instead, but
/// this module and that one do not share a crate boundary that makes one
/// worth minting.
fn well_known(name: &str) -> i64 {
    let text = entry::with_runtime(|context| entry::make_string(context, name));
    entry::key_number(text)
}
