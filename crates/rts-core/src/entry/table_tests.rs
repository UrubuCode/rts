//! The tests of `table.rs`, apart so that file stays under its ceiling.
//!
//! A child of `table` through `#[path]`, as `context_tests.rs` is of `mod.rs`.

use super::*;

#[test]
fn the_numbers_are_written_and_dense() {
    for (position, entry) in CoreEntry::ALL.iter().enumerate() {
        assert_eq!(
            entry.index(),
            position,
            "{entry:?} sits where its number says â€” a list whose numbers \
             came from its order would not survive a removal"
        );
    }
    assert_eq!(CoreEntry::ALL.len(), CORE_ENTRY_COUNT);
}

#[test]
fn every_entry_has_its_own_name() {
    let mut names: Vec<&str> = CoreEntry::ALL.iter().map(|e| e.symbol()).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(names.len(), unique, "two entries sharing a name would link");
}

#[test]
fn a_signature_says_what_the_definition_says() {
    // Not a restatement â€” the definitions below take exactly these, and a
    // change to one without the other stops compiling.
    use rts_cranelift::abi::AbiType;
    use rts_cranelift::repr::Repr;

    assert_eq!(CoreEntry::Add.signature().params.len(), 2);
    assert_eq!(CoreEntry::ToBoolean.signature().params.len(), 1);
    assert_eq!(
        CoreEntry::NumberToString.signature().params,
        vec![AbiType::Scalar(Repr::F64)],
        "a number goes in as a number, not as a tagged value â€” derived from \n             `value: f64`, not written here"
    );
    assert_eq!(
        CoreEntry::Add.signature().params,
        vec![AbiType::Scalar(Repr::Tagged); 2],
        "and a `u64` parameter is a tagged value, which is what a Value is"
    );
}

#[test]
fn the_list_is_short_enough_to_read_in_one_screen() {
    // The membership rule's whole job. If this ever fails, the question is
    // not whether to raise the number â€” it is which of the new entries is
    // arithmetic wearing a call.
    //
    // Asked and answered once, at 64. The three that crossed it are
    // `GeneratorNew` (allocates a frame), `GeneratorYield` (writes context
    // state) and `ModulePublishAll` (walks one namespace and writes
    // another) â€” every one of them touches the heap or global mutable
    // state, so none is arithmetic and the ceiling was what had to move.
    // It moved ONCE and by a little: the next entry to cross it has to
    // answer this question again rather than inherit the answer.
    // Moved to 74 on 2026-08-13 for `ObjectPair` and `ArrayOf`, and the
    // question was answered rather than inherited: both build a heap
    // object, so neither is arithmetic, and both exist to replace THREE
    // and FIVE crossings with one â€” a list that grows to shrink the
    // number of calls is the list doing its job.
    // Moved to 75 on 2026-08-13 for `ThrownAddress`, and the question was
    // answered again rather than inherited: it hands out the address of
    // global mutable state, which is the third clause of the membership
    // rule and not the heap one. It is also the same argument as the line
    // above â€” it exists so that a check stops being a crossing at all,
    // which is the list growing to shrink the number of calls.
    // Moved to 76 on 2026-08-14 for `ElementAt`, and the question was
    // answered again: it reads the heap, so it is not arithmetic. What is
    // new is the REASON a second entry point exists for something
    // `GetIndexed` already answers â€” it exists to ask FEWER questions,
    // because the caller proved them while compiling. A list that grows so
    // that a crossing does less work is the same argument as a list that
    // grows so that there are fewer crossings.
    // Moved to 78 on 2026-08-15 for `EnumerateKeys`, and the question was
    // answered the same way: it walks a layout and allocates, so it is not
    // arithmetic. What is new is that a second entry point exists for
    // something `OwnKeys` looks like it answers â€” it does not, because
    // `for`-`in` walks the chain and object rest must not, and one
    // operation cannot be both without a flag nobody could get right.
    // Moved to 80 on 2026-08-15 for `DelegateStep`, and the answer is the
    // membership rule's THIRD clause rather than the first two: it does not
    // allocate and it does not walk the heap, but it records which iterator
    // the generator being resumed is delegating to â€” global mutable state,
    // and the only way `g.throw(e)` can reach the inner iterator's own
    // `throw`. An emitted call could make the call and could not remember.
    //
    // Moved to 82 on 2026-08-15 for `NewTarget`, and the answer is the
    // third clause again: it neither allocates nor walks the heap, it reads
    // the target stack â€” state the runtime keeps and no instruction can
    // see. The alternative was a bit in the calling convention, which is a
    // machine change every call in the program would pay for so that a
    // meta-property almost no program writes could be read without one.
    //
    // And the argument for the LIST, which the paragraph below asks the
    // next mover to make: this entry is the last one the list should absorb
    // on its present terms. Eighty-two hand-numbered rows are past what a
    // reader checks, and the NUMBER is now the only hand-written part of
    // the row â€” `#[rtse::entry]` derives the definition and the host maps
    // it. A number that three files must agree about by inspection is
    // exactly what "one source, generated views" says a generated view
    // should be answering.
    //
    // The ceiling is a reading limit rather than a capacity one, and it has
    // moved four times in one day. That is a signal about the day and not
    // about the mechanism, but the next move should come with an argument
    // for the LIST rather than for the entry.
    // Moved to 84 on 2026-08-15 for `ImportMeta` and `ModuleImport`, and
    // the entry-level question is not the hard one here: both allocate a
    // heap object â€” an `import.meta` and a promise â€” so neither is
    // arithmetic wearing a call, and both read the specifier table, which
    // is global mutable state. The membership rule admits them twice over.
    //
    // The argument for the LIST, which the paragraph above asks for and
    // which this move does NOT make: there isn't one. Two more hand-numbered
    // rows past a limit already declared unreadable is the mechanism debt
    // being paid again rather than settled, and it is recorded here as debt
    // instead of being dressed up as a reason. What makes the debt bearable
    // and not silent is that these two close the module system's last two
    // holes â€” a program could not write `import.meta` or `import()` at all
    // â€” so the alternative to the rows was a refusal by name, not a
    // different design. The generated view `#[rtse::entry]` already has
    // enough information to produce is the thing that ends this, and the
    // next mover inherits an argument that has now failed to be made twice.
    // Moved to 87 on 2026-08-15 for `ArgumentsObject`. The entry-level
    // question is easy â€” it allocates an object and reads the running
    // call's argument vector, which is global mutable state â€” and the
    // LIST-level argument this ceiling asks for is still not made. What it
    // is NOT is a row bought cheaply: the alternative was leaving
    // `arguments` as an Array, which is a wrong answer to
    // `Array.isArray` and gives every `arguments` a `map` the language
    // says it has not. Reusing `RestArguments` with a sentinel `from` was
    // rejected: one number would then mean two shapes of result, which is
    // the kind of second meaning this list exists to keep out.
    // Moved to 88 on 2026-08-16 for `WithHas`. The entry-level question is
    // easy â€” it walks a prototype chain and reads two properties, which is
    // the heap â€” and the LIST-level argument is still not made. What this
    // row is NOT is a convenience: the alternative was the emitter asking
    // `HasProperty` and then reading `Symbol.unscopables` itself, which
    // puts a scoping rule in the language layer in a second spelling, and
    // the day the two disagree a `with` resolves the wrong binding
    // silently. Reusing `HasProperty` was rejected for the same reason one
    // number must not mean two answers.
    // Moved to 89 on 2026-08-20 for `NumberRemainder`, and this is the one
    // row whose ENTRY-level question is not easy: it is pure computation,
    // which the module header says is instructions. The answer is that the
    // machine provably cannot express it exactly — `NumOp`'s documentation
    // in `rts-cranelift` carries the proof — so the rule's premise, that an
    // instruction was available, does not hold here.
    //
    // The LIST-level argument, which this ceiling exists to force: a reader
    // can still hold the list, and the alternative was not a shorter list.
    // It was `%` never leaving the generic path, which measurement on
    // 2026-08-20 put at 13 proven instructions against 16 generic calls in
    // a body whose every local is annotated `number` — because a local
    // reassigned through `%` was unprovable, which made everything
    // downstream of it unprovable too.
    //
    // Reusing `Remainder` was REJECTED: one number would mean two shapes,
    // tagged both ways for one caller and unboxed both ways for the other.
    //
    // Moved to 95 on 2026-08-28 for `ArrayPatternDirect`. The entry-level
    // question is easy: it reads the elements table, the class registry, two
    // prototypes and their property slots, all of which are global mutable
    // state no instruction reaches.
    //
    // The LIST-level argument, which is the one this ceiling exists to
    // force, and here it is unusually clean, because the alternative EXISTS
    // and was measured. The four questions can be asked from the emitter out
    // of entries that are already on this list — `emit/foreach.rs` asks its
    // own version that way and adds no row. Two things are wrong with it.
    //
    // It costs more than it saves. That shape is a `Symbol.iterator` read,
    // an `ArrayNew` and an identity comparison: 203 ns measured, of which 66
    // is an array allocated only to read a method off it. `for`-`of` pays it
    // once per loop and it disappears; a destructuring pays it once per
    // destructuring, against the ~2 000 ns the fast path is there to remove
    // — and it would put an ALLOCATION on the path whose purpose is to
    // remove one.
    //
    // And it cannot ask the question. Three of the four clauses — an own
    // elements vector, not a proxy, a `next` nobody has replaced — have no
    // spelling in the emitted form at all. `foreach.rs`'s version omits
    // them, and that omission is not hypothetical: `for (const v of new
    // Proxy([1, 2, 3], {}))` throws `TypeError: the value is not iterable`
    // here and answers 6 on node. Copying that guard would have carried the
    // defect into destructuring; one row buys the version that cannot.
    // Raised from 96 to 98 for page_global_get/page_global_set, and to 99
    // for for_in_has, to 101 for iterator_result, to 102 for tail_call
    // to 103 for unary_plus and to 104 for serde_declare — one entry at a time, not the
    // order-of-magnitude jump this ceiling exists to catch (rts-symbol-baker's "thousands" is the
    // shape it refuses).
    //
    // Moved to 106 on 2026-09-19 for `JsonStringify` and `JsonParse`, and the
    // question was asked again: neither is arithmetic — one walks the heap and
    // allocates its answer, the other builds a tree of cells — and both are the
    // `ObjectPair` argument, a row that exists to REMOVE crossings. The call
    // they replace is a global read, a property read through the chain cache
    // and the generic call, three crossings for a function the compiler can
    // name; measured, `JSON.stringify(42)` went from 242 ns to 169.
    //
    // Moved to 107 on 2026-09-26 for `TextWalk`, asked the same way: it is not
    // arithmetic -- it reads two prototypes a program may write to and answers
    // a list it allocates -- and it REMOVES crossings, which is the only
    // argument this list has accepted. A `for`-`of` over a string stepped the
    // iterator, a `next` call and a `{ value, done }` record per character;
    // `bench/analytic.ts` `for-of chars 16` read 432 ns per character through
    // the MIR stage against 181 for the running emitter's list.
    assert!(
        CORE_ENTRY_COUNT <= 107,
        "an explicitly numbered list stops being the right mechanism when \
         nobody can read it"
    );
}
