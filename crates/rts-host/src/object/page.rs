//! Compiling a page's `<script>` bodies into the SAME compilation as the main
//! program, so one object file and one manifest carry both.
//!
//! # Why they must share ONE `FuncRegistry`/`KeyRegistry`, not one each
//!
//! `rts-cranelift::target::place_in_object` places a single `&FuncRegistry`
//! into a single object, once — it is not a batching API, and giving it a
//! second `FuncRegistry` for the page scripts would mean a second object file.
//! `rts-runtime`'s facade is generic and prebuilt: it reads `__rts_functions`,
//! `__rts_frames` and `__rts_modules` by those FIXED names, and a program with
//! zero `--html` files must still link — so a second, differently-named set of
//! tables would have to exist UNCONDITIONALLY, empty when unused, which is
//! exactly the kind of table a real program never needed until a stale
//! assumption about it broke somewhere else.
//!
//! And even before that: `rts-core`'s README rule 3 is why a SEPARATE, unseeded
//! `KeyRegistry` for the scripts is not an option at all — key 0 numbered by
//! two different compilations names two different properties, and `obj.foo`
//! compiled by one would read whatever the other put in slot 0. This module
//! exists to append the scripts into the ONE registry the main program already
//! has, seeded and ordered exactly the way [`crate::live`] seeds a second JIT
//! compilation against a first — the same mechanism, run ahead of time instead
//! of one execution at a time.
//!
//! # The one thing that mechanism cannot give this module for free
//!
//! At JIT time, script `N+1`'s `enclosing` is measured by
//! [`rts_core::entry::environment_names`] off the object script `N` actually
//! wrote to, AFTER it ran. Nothing runs here — every script is placed before
//! any of them executes — so this module builds the same growing list a
//! different way: `emit_page_program` already computes what each script
//! PUBLISHES (its own top-level `var`/`function` names) as a pure fact about
//! its syntax, and hands it back for exactly this reason. Chaining that
//! return into the next call's `enclosing` is sound because publishing is
//! unconditional — every top-level `var`/`function` becomes a global property
//! whether or not the script that declares it ever runs a line, per
//! ECMA-262 §16.1.7 — so the set this module computes without running
//! anything is the same set a JIT run would end up with once script `N` HAD
//! run. What it cannot see is a property a script creates dynamically —
//! `window[computed] = …`, `Object.defineProperty`, `eval` — which is a NAMED
//! cut this crate's docs record rather than a silent gap: `docs/engine/aot-page-scripts.md`.

use rts_codegen::names::Name;
use rts_cranelift::ir::FuncId;
use rts_cranelift::shape::KeyRegistry;

use crate::link::HostError;
use crate::object::html_scripts;
use crate::run::FrontEnd;
use crate::wrap_script::wrap_and_parse_script;

/// Extends `front` with every script in `sources`, in order, and answers what
/// the manifest needs to find each of them again: the hash of its exact
/// source, paired with the [`FuncId`] its compiled entry was placed under.
///
/// A no-op — `front` unchanged, an empty list back — when `sources` is empty,
/// which is every `rts compile` that never named `--html`. Nothing above this
/// function pays for a `<script>` it does not have.
pub(crate) fn extend(
    mut front: FrontEnd,
    sources: &[String],
) -> Result<(FrontEnd, Vec<(u64, FuncId)>), HostError> {
    if sources.is_empty() {
        return Ok((front, Vec::new()));
    }

    // Measured once per `rts compile`, however many scripts and however many
    // `--html` files there are: `WindowImpl`'s own surface does not depend on
    // which page loads into it.
    let window_base = html_scripts::window_base()?;

    // A FRESH `KeyRegistry`, advanced past every key `front.names` already
    // handed out — NOT via `crate::run::reserve_keys`, which is
    // `crate::live`'s tool for the opposite situation: a JIT compilation that
    // joins a RUNNING one starts from an EMPTY `Names`, so asking it to key
    // each seed text mints one call to `KeyRegistry::declare_one` per text,
    // which is how its counter reaches the right count at all.
    //
    // `front.names` here is not empty — it is the MAIN PROGRAM's own, and
    // `Names::key` answers an ALREADY-keyed name from its own map without
    // touching the registry it is handed (that is what makes it safe to call
    // twice on one name). So `reserve_keys` over this `front.names` would call
    // `declare_one` for a name ZERO times — every one of them already keyed —
    // and this fresh registry would still read `issued: 0`, ready to hand
    // page-script emission Key(0) for its first new property, colliding with
    // whichever property the main program already numbered zero. Advancing
    // the counter directly is the fix: `front.names`'s OWN map already holds
    // the right key for every name it has one for, so nothing needs re-asking
    // — the registry only needs to know how many are already spoken for.
    let already_keyed = front.names.keyed_texts().len() as u32;
    let mut keys = KeyRegistry::new();
    keys.declare(already_keyed);

    let mut ctx = rts_codegen::emit::Ctx::new(
        &front.model,
        &mut front.funcs,
        &mut front.calls,
        &mut keys,
        &mut front.names,
        &front.types,
    );

    // Grows by each script's own PUBLISHED names as the loop below compiles
    // it — see this module's own header for why that stands in for a JIT
    // run's `environment_names`.
    let mut enclosing: Vec<(Name, u32)> = window_base
        .iter()
        .map(|text| (ctx.names.intern(text), 0))
        .collect();

    // Re-seeded before EVERY script: `emit_page_program` calls `finish()`
    // internally, and `finish()` resets `ctx.literals` to empty each time — it
    // does not touch `ctx.funcs`/`types`/`keys`/`names`, which is the half
    // that lets this whole function share one registry with `front` at all.
    // Without this, script 2's first new literal would be numbered `0`, the
    // position `front`'s own literal `0` (or script 1's) already holds.
    // `ctx.literal_units` deduplicates by content, so re-seeding the same text
    // twice is a lookup, not a second entry.
    let mut literals_so_far: Vec<Vec<u16>> = front.emitted.literals.clone();

    let mut hashes: Vec<(u64, FuncId)> = Vec::with_capacity(sources.len());

    for source in sources {
        for units in &literals_so_far {
            ctx.literal_units(units);
        }

        let body = wrap_and_parse_script(source, ctx.names)?;
        let (program, published) =
            rts_codegen::emit::emit_page_program(&body, &enclosing, true, &mut ctx).map_err(
                |error| match error {
                    rts_codegen::emit::EmitError::UnboundName(name) => {
                        HostError::Unbound(ctx.names.text(name).to_owned())
                    }
                    other => HostError::from(other),
                },
            )?;

        hashes.push((rts_core::entry::source_hash(source), program.entry));

        front.emitted.functions.extend(program.functions);
        front.emitted.generators.extend(program.generators);
        front.emitted.function_names.extend(program.function_names);
        // The script's own top-level entry has no name of its own — a script
        // is not a declaration, the same reason `rts-host::run`'s `SCRIPT`
        // constant exists rather than a name read off anything. It still
        // needs an entry here: `object::place`'s address table only carries
        // functions this list names, which is how it tells a placed function
        // from a runtime import with no body in this object.
        front
            .emitted
            .function_names
            .push((program.entry, "<script>".to_owned(), 0, false, false));
        // Templates are appended rather than re-seeded — this module's own
        // header names the cost: a tagged template inside one page `<script>`
        // referencing a site by a position another script's `finish()` also
        // reset could read the wrong pieces. `crate::live`'s `Seed` has the
        // identical gap for JIT `eval`/`Function` bodies, for the identical
        // reason (`Seed` carries literals, never templates); closing it is one
        // fix for both, not something owned by page scripts alone.
        front.emitted.templates.extend(program.templates);

        literals_so_far = program.literals;

        for name in published {
            if !enclosing.iter().any(|(held, _)| *held == name) {
                enclosing.push((name, 0));
            }
        }
    }

    front.emitted.literals = literals_so_far;
    Ok((front, hashes))
}
