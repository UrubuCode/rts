//! `getColorDepth`/`hasColors` — an environment heuristic, not a query.
//!
//! # Reuse-check
//!
//! Node itself has no capability query here (§4 of the spec: documented as able
//! to produce false positives AND false negatives), so there is nothing on the
//! host surface or in `rts-cranelift` this could reach for. The prior answer in
//! this repository is the OLD engine's `rts-node/src/tty/detect.rs::color_depth`,
//! which this crate cannot call — it is the engine being replaced. Its
//! PRECEDENCE is reproduced here rather than a second one being invented, since
//! two orderings over the same variables is exactly the drift a reuse-check
//! exists to stop; what is added is the `env` argument, which the old one had no
//! way to take because its fd-shaped ABI carried no object.

use rts_core::entry;

/// One environment read: `env[name]` when the caller passed an object, else the
/// process's own environment.
///
/// # Why `string_in` and not `text_of`
///
/// The second is `ToString`, so the `undefined` of an ABSENT key answered
/// `Some("undefined")` — every key of every env object read as present, and
/// `getColorDepth` short-circuited on `NO_COLOR` every time. That is the
/// coercion-used-as-a-type-test defect, the same one that made
/// `node:worker_threads` cross every number as a string, and the reason
/// `string_in` exists at all.
fn variable(env: u64, name: &str) -> Option<String> {
    if env == entry::undefined_value() {
        return std::env::var(name).ok();
    }
    entry::with_runtime(|context| {
        let value = entry::get_member(context, env, name);
        entry::string_in(context, value)
    })
}

/// Color support in BITS — `1`, `4`, `8` or `24`, Node's own encoding.
///
/// `fd` is the baseline: with no signal in the environment at all, a real
/// terminal still does 16 colors and a pipe does none. Node gates on exactly
/// that, which is why this is not simply an env function.
///
/// **Never call this while holding a borrow** — [`variable`] takes one.
pub(super) fn depth(env: u64, fd: Option<i64>) -> u32 {
    // An EMPTY `NO_COLOR` is not set, per the no-color.org convention Node
    // follows; treating it as set would disable color for anyone who exports
    // the name blank.
    if variable(env, "NO_COLOR").is_some_and(|held| !held.is_empty()) {
        return 1;
    }
    if variable(env, "NODE_DISABLE_COLORS").is_some() {
        return 1;
    }
    if let Some(forced) = variable(env, "FORCE_COLOR") {
        return match forced.as_str() {
            "0" | "false" | "none" => 1,
            "1" | "true" | "" => 4,
            "2" => 8,
            _ => 24,
        };
    }
    let term = variable(env, "TERM").unwrap_or_default().to_lowercase();
    let color_term = variable(env, "COLORTERM").unwrap_or_default().to_lowercase();
    if term == "dumb" {
        return 1;
    }
    if color_term == "truecolor" || color_term == "24bit" {
        return 24;
    }
    if term.contains("256") {
        return 8;
    }
    if !color_term.is_empty()
        || term.contains("color")
        || term.contains("ansi")
        || term.contains("xterm")
        || term.contains("screen")
        || term.contains("vt100")
    {
        return 4;
    }
    match fd.is_some_and(super::platform::is_terminal) {
        true => 4,
        false => 1,
    }
}

/// Whether at least `count` colors are available.
///
/// `1 << depth` rather than a four-row table of color counts: the table was a
/// second encoding of the same fact, and `24` bits does not fit the `u32` a
/// count is asked in.
pub(super) fn has(count: u32, env: u64, fd: Option<i64>) -> bool {
    (1u64 << depth(env, fd)) >= u64::from(count.max(2))
}
