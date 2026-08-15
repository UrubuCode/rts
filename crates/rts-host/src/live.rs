//! Compiling a function from text, into the program that is already running.
//!
//! # What this is that [`crate::run::compile`] is not
//!
//! A second compilation that shares a runtime. `compile` builds a whole program
//! — its own region, its own key numbering, its own literal table — and
//! `evaluate_source` hands back only values that need none of them, which is
//! why it cannot answer `new Function`: a function is a REFERENCE, and a
//! reference belongs to the region that made it.
//!
//! So this compiles into the region that is already there. The code it places
//! addresses the running context's heap, mints property keys the running
//! interner has already issued, and names literals by positions that table
//! already holds — and everything it adds starts past the end of what was
//! there, which is what makes the first program's numbering survive.
//!
//! # The four agreements, from the other side
//!
//! `run.rs` seeds the runtime from the compilation, because *"the compiler's is
//! finished before this one exists"*. Here the order is reversed and each of
//! the four is paid for differently:
//!
//! | agreement | what it costs here |
//! |---|---|
//! | the singleton numbering | nothing — `ValueModel::declare` over a fresh `TagRegistry` is deterministic, so the second compilation numbers them the same way. Checked rather than assumed. |
//! | the property keys | every key the runtime has issued is re-minted into this compilation's `Names`, in order, before the first statement is emitted. O(keys) per call. |
//! | the literal table | every literal already seeded is read back out of the heap as code units and re-registered, in order, so this compilation's own literals start past them. O(text) per call. |
//! | the region | the base and stride are read off the running context and handed to placement as immediates, exactly as `assemble` does for a fresh region. |
//!
//! The two O(n) costs are per `Function(…)` call and are stated rather than
//! hidden: a program that builds functions in a loop pays them each time.
//! Removing them means letting a compilation START its numbering at an offset,
//! which is a change to `rts-codegen`'s `Ctx` rather than something this crate
//! can decide.
//!
//! # What is deliberately absent
//!
//! **Freeing the code.** A placed module is kept for the life of the thread.
//! Nothing tracks whether a function compiled this way is still reachable —
//! that would be the collector knowing about code, which it does not — so a
//! program that compiles in an unbounded loop grows without bound. Named here
//! rather than discovered.
//!
//! **Sharded addressing.** A program compiled by `compile_for(n > 1)` puts a
//! region selector in the low bits of every reference, and the width is in
//! every address computation of the code already running. This refuses rather
//! than placing a second compilation under the single-region addressing, which
//! would build references the first program reads as other cells.

use rts_cranelift::mem::{RegionBase, RegionBases};
use rts_cranelift::target::InMemory;

use crate::run::{Entry, Seed, addressed, front_end_agreeing, place, prepare};

thread_local! {
    /// Every module placed by a run-time compilation on this thread.
    ///
    /// Held and never read, exactly as `Compiled::placed` is: the handle IS the
    /// lifetime of the code, and dropping it would leave a callable naming
    /// unmapped memory. Thread-local rather than process-wide because a context
    /// is, and the code addresses that context's region.
    static PLACED: std::cell::RefCell<Vec<InMemory>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Compiles `function anonymous(<parameters>) { <body> }` into the running
/// context and answers it as a callable value.
///
/// `None` for source that did not parse, emit, verify or place, and for a
/// runtime this cannot compile against — the caller turns that into a
/// `SyntaxError`, which is what a program sees for the same input in Node.
pub(crate) fn compile_function(parameters: &[String], body: &str) -> Option<u64> {
    let agreement = rts_core::entry::agreement();
    // See the module documentation: a second compilation under single-region
    // addressing would build references the running program reads as other
    // cells, and this crate cannot re-derive the selector width the running
    // code was placed with.
    if agreement.region_selector_bits != 0 {
        return None;
    }

    // A SCRIPT that answers the function, rather than the function itself.
    // `front_end` compiles a function body, so a `return` of a function
    // expression is what makes the value come back — and it means the wrapper
    // is the one `compile` already writes rather than a second one this module
    // would have to keep in step with it.
    //
    // The name is `anonymous` because that is what every engine calls a
    // `Function`-built function, and `f.name` is observable.
    let source = format!(
        "return function anonymous({}) {{\n{body}\n}};",
        parameters.join(", ")
    );

    let seed = Seed {
        keys: &agreement.keys,
        literals: &agreement.literals,
    };
    let front = front_end_agreeing(&source, Some(&seed)).ok()?;

    // The first agreement, checked rather than trusted. It costs nothing to be
    // right — both sides declare over a fresh `TagRegistry` and the declaration
    // is deterministic — and a disagreement would be quiet and total, which is
    // the reason `link.rs` gives for stating it at all.
    let singletons = crate::link::singletons_for(&front.model);
    let kinds = crate::link::kinds_for(&front.model);
    if singletons.undefined != agreement.singletons.undefined
        || singletons.null != agreement.singletons.null
        || singletons.hole != agreement.singletons.hole
        || kinds.symbol != agreement.kinds.symbol
        || kinds.bigint != agreement.kinds.bigint
    {
        return None;
    }

    // Everything this compilation minted past what it was seeded with. Only the
    // tail: the prefix is the running interner's own, re-minted here, and
    // interning it again would issue a second number for a name that has one.
    let keyed = front.names.keyed_texts();
    let keys: Vec<String> = keyed
        .get(agreement.keys.len()..)
        .unwrap_or_default()
        .iter()
        .map(|text| (*text).to_owned())
        .collect();

    let prepared = prepare(front.emitted, front.funcs, front.types, front.calls).ok()?;
    let bases = RegionBases::single(
        RegionBase::Immediate(agreement.region_base),
        agreement.region_stride,
    );
    let placed = place(&prepared, bases).ok()?;
    let (function_names, frames) = addressed(&prepared, &placed);
    let address = placed.address_of(prepared.script)?;

    // Before the code is entered, never after: `closure_new` reads the name
    // table to write `.name` and `.length` on the function it builds, and the
    // body's own literals have to be in the table before its first string is
    // read.
    rts_core::entry::adopt(rts_core::entry::Addition {
        keys,
        literals: prepared.emitted.literals.clone(),
        templates: prepared.emitted.templates.clone(),
        frames,
        function_names,
    });

    // Kept before the call rather than after it, so that an entry which does
    // not return leaves the pages mapped rather than freed under a live frame.
    PLACED.with(|held| held.borrow_mut().push(placed));

    // SAFETY: every compiled function is emitted under one convention, which
    // `Entry` spells and `emit_program` builds — including this script, which
    // was placed with a body by the same `place` the whole-program path uses.
    let entry: Entry = unsafe { std::mem::transmute(address) };
    // A script closes over nothing, has no receiver and was passed no
    // arguments. The context is the caller's and is already installed: this is
    // reached from inside a native, which runs with one and holds no borrow.
    let nothing = rts_core::entry::undefined_value();
    Some(entry(nothing, nothing, nothing, nothing, nothing, nothing))
}
