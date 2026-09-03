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

// Split by what a matcher is ABOUT, once the file crossed the 500-line
// ceiling `CLAUDE.md` sets for everything outside the two engine crates —
// same reason `console/` is `format.rs` + `inspect.rs` + `state.rs` rather
// than one file. `equality` is `toBe`/`toEqual` and the truthiness/nullish
// checks that were already here; `order` is the numeric comparisons; `content`
// is the string/array checks. What every matcher shares — `received_of`,
// `negate_if`, `settle`, the two property names — stays HERE, because it is
// what makes `.not` and the first-mismatch rule apply the same way to a
// matcher on either side of the split, not something either side owns.
mod content;
mod equality;
mod order;

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
/// `make_prototype` is memoised by name, so these eighteen callables are built
/// once for the process instead of once per call — which is what makes
/// [`expect`] a single allocation.
const MATCHERS: &[(&str, Provided)] = &[
    ("toBe", equality::to_be),
    ("toEqual", equality::to_equal),
    ("toBeTruthy", equality::to_be_truthy),
    ("toBeFalsy", equality::to_be_falsy),
    ("toBeNull", equality::to_be_null),
    ("toBeUndefined", equality::to_be_undefined),
    ("toBeDefined", equality::to_be_defined),
    ("toBeGreaterThan", order::to_be_greater_than),
    ("toBeGreaterThanOrEqual", order::to_be_greater_than_or_equal),
    ("toBeLessThan", order::to_be_less_than),
    ("toBeLessThanOrEqual", order::to_be_less_than_or_equal),
    ("toBeCloseTo", order::to_be_close_to),
    ("toBeNaN", order::to_be_nan),
    ("toBeFinite", order::to_be_finite),
    ("toContain", content::to_contain),
    ("toStartWith", content::to_start_with),
    ("toEndWith", content::to_end_with),
    ("toHaveLength", content::to_have_length),
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
        // `.not` is a SECOND instance of the same prototype rather than a live
        // getter — there is no host-side "define a getter under this string
        // name" reachable from here (see [`crate::diagnostics_channel`]'s own
        // note on that same gap), so the inverted form is built once, up
        // front, and every matcher reads `NEGATED` off `this` to know which
        // one it is running as.
        let negated = rts_core::entry::make_instance(context, prototype);
        rts_core::entry::put_member(context, negated, RECEIVED, value);
        let flag = rts_core::entry::boolean_value(true);
        rts_core::entry::put_member(context, negated, NEGATED, flag);
        rts_core::entry::put_member(context, object, "not", negated);
        object
    })
}
/// Where `expect` keeps what it was given.
///
/// A name a program is unlikely to write, and reachable if it does — see the
/// module documentation for why that is stated rather than prevented.
const RECEIVED: &str = "__rts_received";
/// Whether this `Expectation` is the `.not` form — absent (reads `undefined`,
/// falsy) on the plain one, so [`is_negated`] needs no default-handling of its
/// own beyond an ordinary boolean coercion.
const NEGATED: &str = "__rts_negated";

/// What `expect` was given, off the receiver.
fn received_of(this: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::get_member(context, this, RECEIVED)
    })
}

/// Whether `this` is the `.not` form of an `Expectation` — see [`NEGATED`].
fn is_negated(this: u64) -> bool {
    let flag = rts_core::entry::with_runtime(|context| rts_core::entry::get_member(context, this, NEGATED));
    rts_core::entry::to_boolean(flag)
}

/// A matcher's own answer, inverted when `this` is the `.not` form — the one
/// line every matcher above shares, so `.not` cannot mean something different
/// for one matcher than for another.
fn negate_if(this: u64, held: bool) -> bool {
    held != is_negated(this)
}

/// Records a mismatch against the running test, and answers `undefined`.
///
/// The FIRST mismatch only: a test whose early expectation failed keeps running
/// here, so every later one would report too, and a list of consequences is
/// harder to read than the cause.
fn settle(held: bool, negated: bool, received: u64, expected: u64) -> u64 {
    if !held {
        let described = format!(
            "expected {}{}, received {}",
            if negated { "NOT " } else { "" },
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

/// `ToNumber`, for a matcher that needs it and is not itself a language
/// operator.
///
/// `rts_core::entry` exports no `ToNumber` directly: the real one,
/// `class_support::to_number`, is `pub(in crate::entry)`, and rightly — it
/// runs `ToPrimitive` first, which can call a `valueOf` written in the
/// program, and calling INTO the program is a discipline `rts-core` owns
/// rather than something every crate downstream should be trusted to get
/// right on its own (its own doc comment names the recursion it stops:
/// `Number.prototype.valueOf` reached through the wrong spelling of this same
/// idea recursed until the stack ran out).
///
/// What IS exported is `subtract` — the actual `-` operator, which performs
/// exactly that conversion on both operands before subtracting. `x - 0` is
/// `ToNumber(ToPrimitive(x))` for every value this matcher set ever sees: a
/// string parses, a number passes through, and there is no BigInt case here
/// to be honest about, since a numeric matcher over a BigInt is not a shape
/// this corpus asks for. Reached through the operator this crate already has
/// rather than a second, private copy of the conversion.
fn to_number(value: u64) -> f64 {
    let zero = rts_core::entry::make_number(0.0);
    let difference = rts_core::entry::subtract(value, zero);
    rts_core::entry::number_of(difference).unwrap_or(f64::NAN)
}

/// Pins the eleven matchers `content.rs`/`order.rs` add — independent of
/// `tests/rts_test_matchers.test.ts`, which exercises the same names through a
/// compiled program and a running host. This runs the extern fns directly
/// against a bare [`Context`], the way `entry::operators`'s own tests do, which
/// is why it can catch a `.not` that a matcher forgot to consult even when
/// nothing downstream of `rts-node` will build.
#[cfg(test)]
mod tests {
    use super::*;
    use rts_core::entry::with_runtime;
    use rts_core::value::{Kinds, Singletons};

    fn singletons() -> Singletons {
        Singletons { undefined: 0, null: 1, hole: 2 }
    }

    /// The numbering a fresh test context uses. `Kinds::in_declaration_order`
    /// is `entry::operators`'s own equivalent — and is `#[cfg(test)]` inside
    /// `rts-core`, so it exists only for THAT crate's own test build, not for
    /// a downstream crate's. Its formula is public, though: the machine
    /// reserves `rts_cranelift::tags::TAG_RESERVED_COUNT` (4) tags and hands
    /// the rest out in order, so a symbol and a bigint are 4 and 5 for the
    /// first program in a process — which is all a fresh [`Context`] ever is
    /// here, since nothing in this module's tests declares one of either.
    fn kinds() -> Kinds {
        Kinds { symbol: 4, bigint: 5 }
    }

    /// Runs a body with a context installed, as a compiled program would —
    /// the same shape `entry::operators`'s own `hosted` uses.
    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let (_context, value) = rts_core::entry::with_context(Context::new(singletons(), kinds()), body);
        value
    }

    fn text(s: &str) -> u64 {
        with_runtime(|context| rts_core::entry::make_string(context, s))
    }

    fn array(values: &[u64]) -> u64 {
        with_runtime(|context| rts_core::entry::make_array_in(context, values.to_vec()))
    }

    /// `expect(value).not` — the same object [`expect`] builds a plain
    /// `Expectation` beside.
    fn negated(value: u64) -> u64 {
        let object = expect(0, 0, value, 0, 0, 0);
        with_runtime(|context| rts_core::entry::get_member(context, object, "not"))
    }

    /// Runs `matcher` as if it were the sole statement of a running `test(…)`,
    /// and answers the mismatch it recorded, if any. Bypasses [`test`]'s own
    /// `body: u64` callable — a native Rust body has nothing to wrap into
    /// one — and instead drives [`RECORD`]/[`RUNNING`] the same way `test`
    /// does, so [`settle`] behaves exactly as it does for a compiled program.
    fn run_matcher(matcher: impl FnOnce()) -> Option<String> {
        reset();
        let at = RECORD.with(|held| {
            held.borrow_mut().push(Reported { name: "pinned".to_owned(), failure: None });
            0
        });
        RUNNING.with(|held| *held.borrow_mut() = Some(at));
        matcher();
        RUNNING.with(|held| *held.borrow_mut() = None);
        record().into_iter().next().and_then(|one| one.failure)
    }

    #[test]
    fn to_contain_matches_a_substring_and_not_rejects_it() {
        hosted(|| {
            let hay = text("hello world");
            assert_eq!(run_matcher(|| { content::to_contain(0, expect(0, 0, hay, 0, 0, 0), text("lo w"), 0, 0, 0); }), None);
            assert!(run_matcher(|| { content::to_contain(0, negated(hay), text("lo w"), 0, 0, 0); }).is_some());
        });
    }

    #[test]
    fn to_contain_on_an_array_uses_same_value_membership() {
        hosted(|| {
            let list = array(&[rts_core::entry::make_number(1.0), rts_core::entry::make_number(2.0)]);
            let two = rts_core::entry::make_number(2.0);
            let three = rts_core::entry::make_number(3.0);
            assert_eq!(run_matcher(|| { content::to_contain(0, expect(0, 0, list, 0, 0, 0), two, 0, 0, 0); }), None);
            assert!(run_matcher(|| { content::to_contain(0, expect(0, 0, list, 0, 0, 0), three, 0, 0, 0); }).is_some());
        });
    }

    #[test]
    fn to_start_and_end_with_read_the_edges_not_the_middle() {
        hosted(|| {
            let s = text("hello world");
            assert_eq!(run_matcher(|| { content::to_start_with(0, expect(0, 0, s, 0, 0, 0), text("hello"), 0, 0, 0); }), None);
            assert!(run_matcher(|| { content::to_start_with(0, expect(0, 0, s, 0, 0, 0), text("world"), 0, 0, 0); }).is_some());
            assert_eq!(run_matcher(|| { content::to_end_with(0, expect(0, 0, s, 0, 0, 0), text("world"), 0, 0, 0); }), None);
        });
    }

    #[test]
    fn to_have_length_parses_the_precomputed_length() {
        hosted(|| {
            let five = text("5");
            let expected = rts_core::entry::make_number(5.0);
            assert_eq!(run_matcher(|| { content::to_have_length(0, expect(0, 0, five, 0, 0, 0), expected, 0, 0, 0); }), None);
            let wrong = rts_core::entry::make_number(6.0);
            assert!(run_matcher(|| { content::to_have_length(0, expect(0, 0, five, 0, 0, 0), wrong, 0, 0, 0); }).is_some());
        });
    }

    #[test]
    fn ordering_matchers_agree_with_a_plain_number_comparison() {
        hosted(|| {
            let ten = text("10");
            let five = rts_core::entry::make_number(5.0);
            assert_eq!(run_matcher(|| { order::to_be_greater_than(0, expect(0, 0, ten, 0, 0, 0), five, 0, 0, 0); }), None);
            assert!(run_matcher(|| { order::to_be_less_than(0, expect(0, 0, ten, 0, 0, 0), five, 0, 0, 0); }).is_some());
            assert_eq!(run_matcher(|| { order::to_be_less_than(0, negated(ten), five, 0, 0, 0); }), None);
        });
    }

    #[test]
    fn to_be_close_to_uses_the_stated_precision_and_defaults_to_two() {
        hosted(|| {
            let sum = text(&format!("{}", 0.1 + 0.2));
            let expected = rts_core::entry::make_number(0.3);
            let precision = rts_core::entry::make_number(10.0);
            assert_eq!(run_matcher(|| { order::to_be_close_to(0, expect(0, 0, sum, 0, 0, 0), expected, precision, 0, 0); }), None);
            // No precision given: the default (2) is loose enough that this still
            // passes, which is the point — a caller who does not ask for
            // precision gets Jest's own default rather than an exact match.
            let absent = rts_core::entry::undefined_value();
            assert_eq!(run_matcher(|| { order::to_be_close_to(0, expect(0, 0, sum, 0, 0, 0), expected, absent, 0, 0); }), None);
        });
    }

    #[test]
    fn to_be_nan_and_to_be_finite_split_on_the_coerced_number() {
        hosted(|| {
            let nan = text("NaN");
            assert_eq!(run_matcher(|| { order::to_be_nan(0, expect(0, 0, nan, 0, 0, 0), 0, 0, 0, 0); }), None);
            assert!(run_matcher(|| { order::to_be_finite(0, expect(0, 0, nan, 0, 0, 0), 0, 0, 0, 0); }).is_some());
            let finite = text("1.5");
            assert_eq!(run_matcher(|| { order::to_be_finite(0, expect(0, 0, finite, 0, 0, 0), 0, 0, 0, 0); }), None);
        });
    }
}
