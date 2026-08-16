//! `await` parks the frame, and the proof is an ORDER rather than a value.
//!
//! Every one of these programs computes the same numbers whether `await`
//! suspends or drains in place. What differs is when each digit is appended, so
//! the answer is a number whose DIGITS are the order things happened in — the
//! one thing a blocking `await` cannot fake.
//!
//! Measured against Bun and Node before it was written: both answer `123` for
//! the first program, and this engine answered `132` for as long as `await`
//! kept the machine.
//!
//! Top-level `await` is what makes a single answer possible at all: a script's
//! own frame drains, so the value returned here is read after the resumption
//! has happened rather than before the queue was ever pumped.

use rts_cranelift::tags;
use rts_host::compile;

/// Runs a script and hands back the double it produced.
fn number(source: &str) -> f64 {
    let mut program =
        compile(source).unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"));
    tags::decode_double(program.run())
}

#[test]
fn the_caller_continues_before_the_half_after_the_await() {
    let order = number(
        "let log = 0;
         async function f() { log = log * 10 + 1; await Promise.resolve(); log = log * 10 + 3; }
         const p = f();
         log = log * 10 + 2;
         await p;
         return log;",
    );
    assert_eq!(
        order, 123.0,
        "the body runs to its first `await`, the CALLER carries on, and the \
         rest of the body follows on a later turn — `132` is the same three \
         statements with the frame never parked"
    );
}

#[test]
fn a_then_attached_after_the_await_runs_after_the_resumption() {
    // The two kinds of waiter — a parked frame and a `.then` callback — share
    // one queue and one identifier space, so their order is the order they
    // attached. Nothing else in the program decides it.
    let order = number(
        "let log = 0;
         const settled = Promise.resolve();
         async function f() { await settled; log = log * 10 + 1; }
         const p = f();
         settled.then(function () { log = log * 10 + 2; });
         await p;
         return log;",
    );
    assert_eq!(order, 12.0, "the `await` attached first, so it resumes first");
}

#[test]
fn a_value_that_is_not_a_promise_still_costs_a_turn() {
    // `await 1` suspends. It is the rule that keeps two chains started in one
    // order from finishing in another, and the shape a blocking implementation
    // gets wrong while computing the right value.
    let order = number(
        "let log = 0;
         async function f() { await 1; log = log * 10 + 2; }
         const p = f();
         log = log * 10 + 1;
         await p;
         return log;",
    );
    assert_eq!(order, 12.0);
}

#[test]
fn a_rejection_crossing_an_await_is_caught_where_it_was_written() {
    // Raised AT the suspension, inside the regions the `await` sits in — which
    // is why a `try` around it catches. Raised anywhere else it would land in
    // the drain's frame, which has no handler the program wrote.
    let answer = number(
        "async function f() {
           try { await Promise.reject(7); return 0; } catch (e) { return e + 1; }
         }
         return await f();",
    );
    assert_eq!(answer, 8.0);
}

#[test]
fn a_throw_escaping_the_body_rejects_the_promise() {
    // It does not end the program, and it does not reach the caller's frame:
    // the completion of an async function is a settlement.
    let answer = number(
        "async function f() { throw 41; }
         try { await f(); return 0; } catch (e) { return e + 1; }",
    );
    assert_eq!(answer, 42.0);
}

#[test]
fn a_local_survives_every_suspension_in_a_loop() {
    // What the frame rewrite spills: a value defined before a suspension and
    // read after it is not in a register any more.
    let answer = number(
        "async function f() {
           let total = 0;
           for (let i = 1; i <= 4; i++) { total += await Promise.resolve(i); }
           return total;
         }
         return await f();",
    );
    assert_eq!(answer, 10.0);
}
