//! `rts napi <file.node>` — load a native addon and say what it exports.
//!
//! # Why this is a command and not a test
//!
//! An addon resolves `napi_*` out of the process that maps it, and only ONE
//! binary in this workspace exports them: `rts` itself. A cargo test is its own
//! executable with its own export table — empty — so a `.node` loaded there
//! would fail on the first symbol no matter how complete the surface is. This
//! is the one place the question can be asked.
//!
//! # What it is for
//!
//! Answering "does this addon load, and if not, what is missing". The loader
//! opens with every symbol resolved eagerly, so a missing one is reported here
//! with its name — which turns "what else does an addon need" from a guess into
//! a list.

use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub fn command(input: Option<String>) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts napi <file.node>"))?;
    let path = PathBuf::from(&input);
    if !path.exists() {
        return Err(anyhow!("no such file: {}", path.display()));
    }

    // SAFETY: mapping arbitrary native code and running its constructors, which
    // is what loading an addon has always meant — the same trust `require` of a
    // `.node` asks for in Node.
    let addon = unsafe { rts_napi::loader::open(&path) }.map_err(|e| anyhow!("{e}"))?;

    // A runtime, because the addon's registrar makes values: it creates
    // strings, hangs functions on an object, and every one of those reaches the
    // thread's context.
    let context = rts_core::entry::Context::new(
        rts_core::value::Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        },
        rts_core::value::Kinds {
            symbol: 4,
            bigint: 5,
        },
    );
    let (_, listed) = rts_core::entry::with_context(context, || {
        let env = rts_napi::Env::new().into_raw();
        // SAFETY: the environment outlives the addon, which is never unloaded.
        let exports = unsafe { addon.exports(env) };
        let exports = exports?;
        let names = rts_core::entry::with_runtime(|runtime| {
            rts_core::entry::member_names(runtime, exports)
        });

        // Each export is CALLED when it takes no arguments, because listing
        // names proves the registrar ran and nothing more. What an addon is
        // for is answering something, and a call is the only thing that shows
        // the whole path — the trampoline, the handle scope, the value coming
        // back out.
        let called: Vec<(String, Option<String>)> = names
            .into_iter()
            .map(|name| {
                let member = rts_core::entry::with_runtime(|runtime| {
                    rts_core::entry::get_member(runtime, exports, &name)
                });
                let callable = rts_core::entry::with_runtime(|runtime| {
                    rts_core::entry::is_callable_in(runtime, member)
                });
                if !callable {
                    return (name, None);
                }
                let nothing = rts_core::entry::undefined_value();
                let arguments = rts_core::entry::make_array(Vec::new());
                let produced = rts_core::entry::call_with_args(member, nothing, arguments);
                // A throw is reported as the answer rather than swallowed: an
                // addon that raises has told us something, and printing
                // `undefined` would hide it.
                let answer = match rts_core::entry::pending() {
                    Some((_, described)) => format!("threw: {described}"),
                    None => rts_core::entry::text_of(produced)
                        .unwrap_or_else(|| "(not text)".to_owned()),
                };
                (name, Some(answer))
            })
            .collect();
        Some(called)
    });

    let Some(names) = listed else {
        return Err(anyhow!(
            "{} loaded, but its registrar produced nothing",
            path.display()
        ));
    };
    println!("{} loaded, exporting {} names:", path.display(), names.len());
    for (name, answer) in names {
        match answer {
            Some(answer) => println!("  {name}() -> {answer}"),
            None => println!("  {name}"),
        }
    }
    Ok(())
}
