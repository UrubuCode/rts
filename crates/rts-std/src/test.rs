//! `rts:test` — `describe`, `test`, and `expect`.
//!
//! # What a failed expectation does here, and what it does not
//!
//! It **records** and the test body keeps running. The real harness throws, so
//! the statements after a failed `expect` do not run there and do run here.
//!
//! That is a divergence rather than a design: throwing across a call needs the
//! machine's protected regions on the call path, which `rts-host`'s plan
//! lists as not wired. Recording is the answer available today, and it fails in
//! the direction that can be read — a test reports the first mismatch it saw —
//! rather than the direction that lies, which would be reporting a pass because
//! nothing stopped.
//!
//! # Why the received value is an ordinary property
//!
//! `expect(x)` answers an object whose matchers need `x`. It is stored as a
//! property, reachable from the program, rather than beside the cell: state
//! beside a cell is `rts-core`'s mechanism and is not exported, and a test
//! harness reading its own scratch property is not a soundness question. Named
//! rather than hidden.

use std::cell::RefCell;

use rts_core::entry::{Context, Provided};

/// What one `test(…)` reported.
pub struct Reported {
    /// The name it was given.
    pub name: String,
    /// The first mismatch, if any. `None` is a pass.
    pub failure: Option<String>,
}

thread_local! {
    /// What the program has reported so far, in the order it reported it.
    ///
    /// Thread-local because a context is: `rts-core` holds one per thread
    /// and a table shared across them would need a lock on a path that has no
    /// contention to justify one.
    static RECORD: RefCell<Vec<Reported>> = const { RefCell::new(Vec::new()) };
    /// The test currently running, so a failed expectation knows what to blame.
    static RUNNING: RefCell<Option<usize>> = const { RefCell::new(None) };
}

/// Everything reported since the last [`reset`], in order.
pub fn record() -> Vec<Reported> {
    RECORD.with(|held| {
        held.borrow()
            .iter()
            .map(|one| Reported {
                name: one.name.clone(),
                failure: one.failure.clone(),
            })
            .collect()
    })
}

/// Empties the record, for a host running more than one program.
pub fn reset() {
    RECORD.with(|held| held.borrow_mut().clear());
    RUNNING.with(|held| *held.borrow_mut() = None);
}

/// The namespace `rts:test` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("describe", describe),
        ("test", test),
        ("it", test),
        ("expect", expect),
    ];
    rts_core::entry::make_namespace(context, members)
}

/// `describe(name, body)` — runs the body, groups nothing yet.
///
/// Grouping is a presentation decision and belongs to whatever reads the record;
/// what matters for running the corpus is that the body runs, because every test
/// in it is written inside one.
extern "C" fn describe(_e: u64, _this: u64, _name: u64, body: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    rts_core::entry::call(body, absent, absent, absent, absent, absent)
}

/// `test(name, body)` — runs the body and records what it reported.
extern "C" fn test(_e: u64, _this: u64, name: u64, body: u64, _a2: u64, _a3: u64) -> u64 {
    let title = rts_core::entry::described(name).unwrap_or_default();
    let at = RECORD.with(|held| {
        let mut held = held.borrow_mut();
        held.push(Reported {
            name: title,
            failure: None,
        });
        held.len() - 1
    });
    // Which test a later mismatch belongs to. Restored rather than cleared,
    // because a `test` inside a `test` is legal and the outer one is still
    // running when the inner finishes.
    let outer = RUNNING.with(|held| held.borrow_mut().replace(at));
    let absent = rts_core::entry::undefined_value();
    let answered = rts_core::entry::call(body, absent, absent, absent, absent, absent);
    RUNNING.with(|held| *held.borrow_mut() = outer);
    answered
}

/// The matchers, on ONE prototype every `expect` shares.
///
/// `make_prototype` is memoised by name, so these seven callables are built
/// once for the process instead of once per call — which is what makes
/// [`expect`] a single allocation.
const MATCHERS: &[(&str, Provided)] = &[
    ("toBe", to_be),
    ("toEqual", to_be),
    ("toBeTruthy", to_be_truthy),
    ("toBeFalsy", to_be_falsy),
    ("toBeNull", to_be_null),
    ("toBeUndefined", to_be_undefined),
    ("toBeDefined", to_be_defined),
];

/// `expect(value)` — an object carrying the value and the matchers.
///
/// # One allocation, and why that is correctness rather than economy
///
/// This used to make a plain object and then, in a second borrow, hang SEVEN
/// freshly built callables off it. The object lived in a Rust local across all
/// of them, and every one of those `make_callable`/`put_member` calls can
/// collect — which is hiding place two of `docs/engine/lost-roots.md`, the one
/// `json::materialise` was caught by: *a native building a cell out of a Rust
/// local*. The guard for it, `entry::rooted::Rooted`, is `pub(in crate::entry)`
/// and this crate cannot reach it.
///
/// A shared prototype removes the window instead of guarding it. `make_instance`
/// is the only allocation between having nothing and having a complete object,
/// so there is no longer a half-built cell to lose. The failure this fixes was
/// visible and misleading: under memory pressure the suite reported
/// `TypeError: (intermediate value).toBe is not a function` — the matcher
/// object had been swept between `expect(x)` and reading `.toBe` off it, and
/// the file that crashed was never the file at fault.
///
/// It is also seven allocations cheaper per assertion, which a suite of three
/// thousand assertions pays three thousand times. That is the smaller half.
extern "C" fn expect(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        let prototype = rts_core::entry::make_prototype(context, "Expectation", MATCHERS);
        let object = rts_core::entry::make_instance(context, prototype);
        rts_core::entry::put_member(context, object, RECEIVED, value);
        object
    })
}
/// Where `expect` keeps what it was given.
///
/// A name a program is unlikely to write, and reachable if it does — see the
/// module documentation for why that is stated rather than prevented.
const RECEIVED: &str = "__rts_received";

/// `expect(a).toBe(b)`.
///
/// Jest's `toBe` is `Object.is`, not `===`: `expect(0/0).toBe(NaN)` passes and
/// `expect(-0).toBe(0)` fails in the real harness, which `===` gets backwards on
/// both. `rts_core::entry::same_value` is `SameValue` for exactly that
/// reason — this used to call `strict_equals` and silently graded both cases
/// the wrong way.
///
/// `toEqual` is installed as the same function. They differ in the language's
/// harness — one is identity and one is deep equality — and a deep comparison
/// that ran on identity would report passes it did not earn, so the two share
/// the same-value answer and the divergence is named here rather than guessed
/// at.
extern "C" fn to_be(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let held = rts_core::entry::same_value(received, expected);
    settle(held, received, expected)
}

/// `expect(x).toBeTruthy()`.
extern "C" fn to_be_truthy(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let held = rts_core::entry::to_boolean(received);
    settle(held, received, received)
}

/// `expect(x).toBeFalsy()`.
extern "C" fn to_be_falsy(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let held = !rts_core::entry::to_boolean(received);
    settle(held, received, received)
}

/// `expect(x).toBeNull()`.
extern "C" fn to_be_null(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let null = rts_core::entry::null_value();
    settle(received == null, received, null)
}

/// `expect(x).toBeUndefined()`.
extern "C" fn to_be_undefined(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let absent = rts_core::entry::undefined_value();
    settle(received == absent, received, absent)
}

/// `expect(x).toBeDefined()`.
extern "C" fn to_be_defined(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = received_of(this);
    let absent = rts_core::entry::undefined_value();
    settle(received != absent, received, absent)
}

/// What `expect` was given, off the receiver.
fn received_of(this: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::get_member(context, this, RECEIVED)
    })
}

/// Records a mismatch against the running test, and answers `undefined`.
///
/// The FIRST mismatch only: a test whose early expectation failed keeps running
/// here, so every later one would report too, and a list of consequences is
/// harder to read than the cause.
fn settle(held: bool, received: u64, expected: u64) -> u64 {
    if !held {
        let described = format!(
            "expected {}, received {}",
            rts_core::entry::described(expected).unwrap_or_else(|| "an object".to_owned()),
            rts_core::entry::described(received).unwrap_or_else(|| "an object".to_owned())
        );
        RUNNING.with(|running| {
            if let Some(at) = *running.borrow() {
                RECORD.with(|record| {
                    let mut record = record.borrow_mut();
                    if let Some(one) = record.get_mut(at)
                        && one.failure.is_none()
                    {
                        one.failure = Some(described);
                    }
                });
            }
        });
    }
    rts_core::entry::undefined_value()
}
