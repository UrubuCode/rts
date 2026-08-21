//! Reading back a captured variable the emitter just wrote.
//!
//! `emit/binding.rs` answers a read of a captured name with the value it just
//! stored, when the read is the very next thing emitted. That is a memo over
//! MEMORY, and a memo over memory that is spent one moment too late does not
//! produce slow code — it produces a **wrong answer**, silently, on a program
//! that looks right.
//!
//! So most of this file is about when it must NOT fire. The window is narrow
//! by construction (`FuncBuilder::nothing_emitted_here`), and these are the
//! programs that would catch it having been widened by accident.

use rts_cranelift::tags;
use rts_host::compile;

fn number(source: &str) -> f64 {
    let mut program =
        compile(source).unwrap_or_else(|error| panic!("compiling failed: {error:?}"));
    tags::decode_double(program.run())
}

#[test]
fn a_captured_variable_read_straight_after_a_write_answers_what_was_written() {
    // The shape the optimisation exists for, and the one `nextRandomF64` in
    // bench/monte_carlo_pi.ts is written in: assign, then test what was
    // assigned. `peek` is what makes `state` captured; it is never called
    // inside the loop.
    let source = "
        let state = 0;
        function peek() { return state; }
        function step() {
            state = 7;
            if (state < 10) { state = state + 1; }
            return state;
        }
        return step();
    ";
    assert_eq!(number(source), 8.0);
}

#[test]
fn a_call_between_the_write_and_the_read_is_not_forwarded_across() {
    // The case that decides whether the window is sound. `bump` mutates the
    // SAME captured variable, so a memo still standing after the call would
    // answer 1 where the program says 2 — and nothing about the source would
    // look wrong.
    let source = "
        let state = 0;
        function bump() { state = state + 1; return 0; }
        function run() {
            state = 1;
            bump();
            return state;
        }
        return run();
    ";
    assert_eq!(
        number(source),
        2.0,
        "a call can run user code, and this one changes the very variable a \
         forwarded read would have answered from"
    );
}

#[test]
fn an_operator_between_the_write_and_the_read_is_not_forwarded_across() {
    // Subtler than a call: `o * 1` on an object is a runtime operator whose
    // SLOW path calls `valueOf`, which is user code, which here writes the
    // captured variable. The emitter sees an operator, not a call.
    let source = "
        let state = 0;
        const sneaky = { valueOf() { state = 99; return 1; } };
        function run() {
            state = 1;
            const ignored = sneaky * 1;
            return state + ignored - 1;
        }
        return run();
    ";
    assert_eq!(
        number(source),
        99.0,
        "`valueOf` ran between the write and the read and changed the variable"
    );
}

#[test]
fn a_branch_between_the_write_and_the_read_is_not_forwarded_across() {
    // Control flow moved, so the block the memo named is not the block the
    // read is in. If the memo ignored the block it would answer 1 on both
    // paths.
    let source = "
        let state = 0;
        function peek() { return state; }
        function run(flag) {
            state = 1;
            if (flag) { state = 5; }
            return state;
        }
        return run(true);
    ";
    assert_eq!(number(source), 5.0);
}

#[test]
fn two_bindings_of_one_name_at_different_depths_do_not_forward_into_each_other() {
    // The reason `CapturedWrite` carries `hops`. Both bindings are spelled
    // `v`, both are captured — by `outer` and by `inner` — and they are
    // different variables at different depths. A memo keyed on the name alone
    // would answer one for the other.
    //
    // # Why the shadowing is between FUNCTIONS and not inside a block
    //
    // The block form of this test — `{ let v = 10; function inner() {…} }`
    // nested inside a function whose enclosing scope also captures a `v` —
    // answers `NaN` on this engine, and answered `NaN` before this file
    // existed. That is a real defect and it is NOT this optimisation's:
    // verified against the binary of the previous commit, which gives the same
    // `NaN` where Node and Bun both give 21. It is recorded in the commit that
    // added this file rather than pinned by a failing test here, because a
    // test that fails is not a record of anything — it is a broken build.
    let source = "
        let v = 1;
        function outer() { return v; }
        function mid() {
            let v = 10;
            function inner() { return v; }
            v = 20;
            return v + inner();
        }
        return mid() + v;
    ";
    assert_eq!(
        number(source),
        41.0,
        "the inner `v` is 20, read twice, and the outer one is still 1"
    );
}

#[test]
fn the_loop_that_motivated_this_still_computes_the_same_sequence() {
    // The LCG from bench/monte_carlo_pi.ts, reduced. Its `if (state < 0)`
    // reads immediately after a write, which is exactly the forwarded read,
    // and the answer has to be the one Node and Bun give.
    let source = "
        let state = 1;
        function peek() { return state; }
        function run() {
            let i = 0;
            while (i < 3) {
                state = (state * 1664525 + 1013904223) % 4294967296;
                if (state < 0) { state = state + 4294967296; }
                i = i + 1;
            }
            return peek();
        }
        return run();
    ";
    // Taken from Node and Bun running the same recurrence, not worked out by
    // hand — a hand-worked value for this exact sequence was wrong once
    // already, and the engine was right.
    assert_eq!(number(source), 2165703038.0);
}

#[test]
fn a_captured_binding_written_by_destructuring_answers_a_javascript_value() {
    // The regression the cross-runtime gate caught, as a test.
    //
    // The first version of this optimisation remembered the value HANDED to
    // the store rather than the value the store answered, with a comment
    // arguing the two were interchangeable. They are not: a store widens its
    // argument, and destructuring hands it a `Repr::I64` index. A later read
    // then answered the raw index where a JavaScript value was required, and
    // the compiler stopped with `Place(Lower(CannotWiden { from: I64 }))` —
    // `tests/cross-runtime/iteration/claude-destructuring-lazy-iterator-pull.ts`
    // went from passing to failing to compile.
    //
    // This is the shape reduced to what breaks: destructure into a CAPTURED
    // binding, then read it back immediately.
    let source = "
        let first = 0;
        let rest = 0;
        function peek() { return first + rest; }
        function run() {
            [first, rest] = [3, 4];
            return first + rest;
        }
        return run() + peek();
    ";
    assert_eq!(number(source), 14.0);
}

#[test]
fn a_closure_made_between_the_write_and_the_read_still_sees_the_written_value() {
    // The other direction: the memo must not let the emitter skip the STORE.
    // If the write were elided rather than merely read back, the closure would
    // observe the old value.
    let source = "
        let state = 0;
        function run() {
            state = 42;
            const read = function () { return state; };
            return read();
        }
        return run();
    ";
    assert_eq!(
        number(source),
        42.0,
        "the value has to be in the environment, not only in a register — a \
         closure reads the slot"
    );
}
