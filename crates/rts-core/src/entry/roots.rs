//! Phase 2b: what is live *from outside the heap* at the moment a collection
//! runs.
//!
//! # What this does not do
//!
//! It does not mark and it does not sweep — [`crate::collect`] does both, over
//! whatever slots arrive. And it does not derive a PRECISE root set:
//! `rts_cranelift::gc::describe_frames` exists for that, is finished and tested,
//! and has zero callers — nothing attaches its liveness output to compiled code
//! at run time, so a frame the machine compiled cannot be asked which of its
//! slots are live. That is a separate, larger piece of work, and this module
//! does not attempt it.
//!
//! What is left, and what this module answers, is two questions:
//!
//! **(A) What does the [`Context`] itself hold live** — not because some other
//! object points at it, but because the runtime holds it directly: the call
//! stack, the pending arguments, the throw in flight, the cached prototypes,
//! the promise machine's reactions. [`context_roots`] enumerates every field of
//! `Context` and answers.
//!
//! **(B) What does a stack this engine did not annotate hold live** — a
//! compiled frame's spill slot or callee-saved register that never made it into
//! (A), and a Rust or foreign frame calling into the runtime, which the
//! precise pipeline (were it wired) could never describe because it has no
//! frame descriptor. [`scan_stack`] answers by treating every word in a given
//! range as a candidate and keeping only the ones that are unambiguously
//! references — see its own documentation for what it deliberately does not
//! do and why.
//!
//! # Why the filter is `Value::kind`, reused rather than restated
//!
//! [`crate::collect::conservative_roots`] already states the rule this module
//! needs twice over — a word is a root only if it is an encoded value *and*
//! its tag says reference — and restating it here would be exactly the "rule
//! written twice" this crate's rule 7 warns about disagreeing eventually. Both
//! (A) and (B) below reduce to calling it.

use crate::collect::conservative_roots;
use crate::heap::Slot;

use super::Context;

/// Every slot the [`Context`] holds live on its own account.
///
/// # The classification behind this list
///
/// Every field of `Context` was read and sorted into three kinds, and only the
/// first is here:
///
/// - **Root** — live because the runtime holds it, not because another object
///   points at it: `callees` (the call stack), `pending_arguments`,
///   `new_targets`, `thrown` (a throw in flight — an in-flight exception's
///   payload has no other holder while it propagates, and missing it here is
///   exactly [the old engine's `53be596d`][scars] shape: a live reference the
///   scanner never enumerates), `yielded` (a generator's last `yield`, live
///   only between the yield and whoever resumes reading it), `literals`,
///   `templates`, `modules`, the cached prototypes (`globals`,
///   `string_prototype`, `array_prototype`, `regexp_prototype`), `classes`
///   (each entry's `made` and its `prototype`), and the promise machine's
///   reactions, settlements and combinator state (`promises`, via
///   [`super::promise::state::Machine::root_words`] — see there for why none
///   of it is reachable from any cell).
/// - **Heap content, not a root** — every `Aside<T>` (`prototypes`, `callables`,
///   `proxies`, `cursors`, `bound`, `accessors`, `boxed`, `generators`,
///   `regexes`, `views`, `collections`, `integrity`, `attributes`,
///   `array_elements`, `spill_of`, `buffer_of`) is keyed **by cell** — see
///   `heap::aside`'s own documentation. Something in it is reachable once the
///   owning cell is marked, through that cell's `Trace` implementation
///   (another agent's phase), and is not a root on its own. The same is true
///   of `spills`, `buffers` and `arrays`, which are reached from a cell via one
///   of those `Aside` tables, and of `region`, the aggregate storage a shape's
///   own trace walks.
/// - **No references at all** — `shapes`, `keys`, `interner`, `types`,
///   `shape_of_type`, `text_type`, `symbols` (a symbol is a machine *tag*, not
///   a cell — see `entry::symbol`'s own module documentation for why that
///   stopped being true the day symbols became primitives), `pending_call_name`
///   (an index into `literals`, not a value), `derived`, `class_constructors`
///   (flags), `barriers`, `resolves` (counters), `remembered` (cell/region
///   pairs describing a store, not a value — and always empty today, per its
///   own module doc), `bigints` (digits, not `Value`s), `function_names`,
///   `frames` (layout metadata, keyed by code address), `evaluator` (a bare
///   `fn` pointer), `loop_sources` and `rest` (a source is a bare
///   `fn() -> Pending`, per `entry::loops`'s own documentation — it closes over
///   nothing, so nothing here can be holding a `Value` on its behalf; if a
///   module's own background state does, that module's table is a root this
///   crate cannot see and is a gap for whoever owns that module to close),
///   `singletons`, `kinds`.
///
/// [scars]: https://github.com/UrubuCode/rts (commit `53be596d`: a receiver
/// reachable only through a masked/narrowed word became invisible once the
/// optimizer hoisted the mask — the same lesson this list exists to avoid:
/// a root source nobody enumerated)
///
/// # Why raw words go through the same filter as a stack word
///
/// Most of these fields already store an unambiguous cell index (`globals`,
/// `string_prototype`, `array_prototype`, `regexp_prototype` are `Option<u32>`,
/// a bare slot with no tag to check) and are turned into a [`Slot`] directly.
/// The rest (`callees`, `pending_arguments`, `new_targets`, `literals`, a
/// class's `made` and `prototype`, …) store encoded `Value` words — the same
/// representation a stack word has — and are run through
/// [`conservative_roots`] exactly as a stack word is, because that is what they
/// are: a `Value` that happens not to be sitting on the machine stack. An
/// integer or a `Client` tag among them is rejected the same way a stray
/// integer on the stack is.
pub fn context_roots(context: &Context) -> Vec<Slot> {
    let mut roots: Vec<Slot> = Vec::new();

    // Bare cell indices — no tag to check, because nothing else is ever stored
    // in these fields.
    roots.extend(
        [
            context.globals,
            context.string_prototype,
            context.array_prototype,
        ]
        .into_iter()
        .flatten()
        .map(Slot),
    );

    // Encoded `Value` words, filtered exactly as a stack word is.
    // `regexp_prototype`, unlike its three siblings above, is stored as a full
    // encoded `Value` (`entry::regex::prototype_of` writes it with
    // `Value::from_slot(cell).bits()`) rather than a bare cell index, so it
    // belongs in this half, not the one above.
    let mut words: Vec<u64> = Vec::new();
    words.extend(context.regexp_prototype);
    words.extend_from_slice(&context.callees);
    words.extend_from_slice(&context.pending_arguments);
    words.extend_from_slice(&context.new_targets);
    words.extend_from_slice(&context.literals);
    // What a native kept after its call returned. The stack scan cannot see
    // these: they live in the holder's own memory, not on a frame. See
    // `super::external`.
    words.extend(context.external.iter().map(|(_, value)| *value));
    words.extend(context.yielded);
    words.extend(context.thrown.map(|(_, payload)| payload));
    words.extend(
        context
            .templates
            .iter()
            .filter_map(|(_, strings)| *strings),
    );
    // The `typeof` answers built so far. Nothing else holds them — a program
    // that drops the string `typeof x` produced keeps no reference, and the
    // cache is what makes the next `typeof` hand back the same cell — so
    // missing them here is a use-after-free that reproduces only after a
    // collection between two `typeof`s.
    words.extend(context.type_names.iter().flatten().copied());
    // And the strings built for the runtime's own use -- `toJSON` and the empty
    // key `JSON.stringify` starts from. Same argument as the line above: the
    // cache is the only holder, so a collection between two uses would free
    // what the next use hands back.
    words.extend(context.well_known_texts.iter().flatten().copied());
    // Only the ones a program has actually named: a module registered lazily
    // has no object yet, and there is nothing to keep alive until it does.
    words.extend(context.modules.iter().filter_map(|held| held.namespace));
    words.extend(
        context
            .classes
            .iter()
            .flat_map(|entry| [entry.made, entry.prototype]),
    );
    words.extend(context.promises.root_words());

    roots.extend(conservative_roots(&words));
    roots
}

/// The slots a range of raw machine words could be referring to.
///
/// For a frame this engine did not compile — a Rust frame, a foreign caller,
/// or a compiled frame whose spill slots and callee-saved registers [`Context`]
/// itself has no field for. **This is the whole of what (B) can be, given that
/// nothing wires `describe_frames`'s precise output to a live frame**: every
/// word in `[low, high)` is a candidate, and [`conservative_roots`] rejects
/// everything that is not unambiguously an encoded reference — a double, an
/// integer, a boolean and a singleton are rejected by their tag, and a real
/// reference word that happens to be dead is merely retained one extra cycle.
///
/// # Safety
///
/// `low` and `high` must describe a range of readable, `u64`-aligned-or-tolerant
/// memory that will not be unmapped or otherwise invalidated while this runs —
/// typically a snapshot of one thread's stack between its current stack
/// pointer and the top the OS gave it. Passing a range that is not actually
/// stack memory, or scanning a suspended thread's range while it is not
/// actually suspended, is undefined behaviour; this function does not and
/// cannot check either.
///
/// # What this deliberately does not do, and why
///
/// **It does not find `low` and `high` for you.** Reading the current stack
/// pointer is a machine fact (an architecture, not an OS), but the *top* of a
/// thread's stack is not: on Windows it is `GetCurrentThreadStackLimits`, on
/// Linux it is `/proc/self/maps`, and both are operating-system calls. This
/// crate's own rule 1 draws the line exactly there — "anything needing an
/// operating system goes to `rts-host`… neither is a dependency of this" — and
/// `rts-core` is bound to exist on wasm, where neither call exists at all.
/// So this function takes the bounds as parameters instead of discovering them,
/// and whoever calls it — necessarily a host-level crate, since the call site
/// is inside `alloc`'s runtime-call boundary — is where the OS call belongs.
///
/// The old engine's `crates/rts-natives/src/collector/cycle.rs` /
/// `collector/scan.rs` is the precedent, not the source: it walks
/// `rsp..stack_high` with `stack_high` from `GetCurrentThreadStackLimits` on
/// Windows (documented there as the correct call — not `gs:[0x10]`/TIB, which
/// can read below `rsp`) or `/proc/self/maps` on Linux, plus callee-saved
/// registers captured before the scan (a value newly created by `alloc` can be
/// live in a register alone, not yet spilled) and other RTS threads suspended
/// via `SuspendThread`/`GetThreadContext`. All of that is sound *reasoning*
/// this module inherits; none of the *code* is reusable as-is, because it is
/// keyed to a generation-tagged handle representation this engine does not
/// have. **On this platform (Windows x86-64, this session's target) obtaining
/// the bounds honestly is possible and the precedent above is confident about
/// it. It is not done here** because doing it here would put an OS call in a
/// crate whose whole membership rule is not needing one — it belongs beside
/// wherever `alloc`'s collection trigger ends up calling from.
pub unsafe fn scan_stack(low: usize, high: usize) -> Vec<Slot> {
    if high <= low {
        return Vec::new();
    }
    let mut words: Vec<u64> = Vec::with_capacity((high - low) / 8);
    let mut addr = low;
    while addr + 8 <= high {
        // SAFETY: the caller's contract is that `[low, high)` is readable,
        // `u64`-tolerant memory. A misaligned read is intentional — a stack
        // offset is not guaranteed 8-byte aligned at every word this walks —
        // so this reads through a `read_unaligned`, matching what a scanner
        // over arbitrary machine words must do.
        let word = unsafe { (addr as *const u64).read_unaligned() };
        words.push(word);
        addr += 8;
    }
    conservative_roots(&words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Kinds, Singletons, Value};

    fn empty_context() -> Context {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        };
        Context::new(singletons, Kinds::in_declaration_order())
    }

    #[test]
    fn context_roots_include_the_globals_object() {
        let mut context = empty_context();
        context.globals = Some(7);
        let roots = context_roots(&context);
        assert!(
            roots.contains(&Slot(7)),
            "the global object is held live by the runtime, not by any other \
             object — a context that forgot it would let the globals object \
             be swept out from under `globalThis`"
        );
    }

    #[test]
    fn context_roots_include_the_call_stack_and_the_throw_in_flight() {
        let mut context = empty_context();
        context.callees.push(Value::from_slot(3).bits());
        context.thrown = Some((0, Value::from_slot(9).bits()));
        let roots = context_roots(&context);
        assert!(roots.contains(&Slot(3)), "the running callable");
        assert!(
            roots.contains(&Slot(9)),
            "an exception propagating between native frames has no other \
             holder — the shape of the old engine's `9c7b5c58` scar, a root \
             source nobody enumerated"
        );
    }

    #[test]
    fn context_roots_reject_a_non_reference_word_in_a_value_field() {
        let mut context = empty_context();
        context.callees.push(Value::from_i32(42).bits());
        context.new_targets.push(Value::from_f64(1.5).bits());
        assert!(
            context_roots(&context).is_empty(),
            "an integer and a double are not references, on the call stack \
             field exactly as on the machine stack"
        );
    }

    #[test]
    fn a_word_on_the_rust_stack_is_found_by_the_scan() {
        let live = Value::from_slot(11).bits();
        let mut frame = [0u64; 4];
        frame[1] = live;
        let low = frame.as_ptr() as usize;
        let high = low + core::mem::size_of_val(&frame);
        let roots = unsafe { scan_stack(low, high) };
        assert!(
            roots.contains(&Slot(11)),
            "a value sitting in a stack slot is exactly what a frame this \
             engine did not annotate looks like"
        );
    }

    #[test]
    fn a_float_or_an_int_on_the_stack_is_not_reported() {
        let mut frame = [0u64; 4];
        frame[0] = Value::from_f64(2.5).bits();
        frame[1] = Value::from_i32(-1).bits();
        let low = frame.as_ptr() as usize;
        let high = low + core::mem::size_of_val(&frame);
        let roots = unsafe { scan_stack(low, high) };
        assert!(
            roots.is_empty(),
            "a double and a small integer are rejected by their tag — the same \
             rule that stops a stray bit pattern from being followed as a \
             reference"
        );
    }

    #[test]
    fn an_empty_or_inverted_range_scans_nothing() {
        assert!(unsafe { scan_stack(10, 10) }.is_empty());
        assert!(unsafe { scan_stack(10, 4) }.is_empty());
    }

    #[test]
    fn an_out_of_range_slot_is_not_rejected_here_but_by_the_mark_that_follows() {
        // `conservative_roots` — and therefore both halves of this module —
        // does not itself know how large the region is: a `Slot` naming a cell
        // that was never allocated is indistinguishable, from a raw word alone,
        // from one that was freed after this scan started. `collect::mark`
        // already handles both: `slab.at(slot)` answers `Err` and the walk
        // skips it rather than treating it as an error. Duplicating a bounds
        // check here would be a second answer to a question `mark` already
        // answers, which is what this crate's rule 7 warns against.
        let mut frame = [0u64; 1];
        frame[0] = Value::from_slot(u32::MAX).bits();
        let low = frame.as_ptr() as usize;
        let high = low + core::mem::size_of_val(&frame);
        let roots = unsafe { scan_stack(low, high) };
        assert_eq!(
            roots,
            vec![Slot(u32::MAX)],
            "produced as a candidate — validity is `mark`'s question, not this \
             module's"
        );
    }
}
