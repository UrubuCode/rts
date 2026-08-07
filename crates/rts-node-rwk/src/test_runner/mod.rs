//! `node:test` — `test`/`it`/`describe`/`suite`, the four hooks, and a
//! per-thread record. Named `test_runner` because `test` collides with
//! cargo's own test-harness module conventions (see the task that assigned
//! this crate area).
//!
//! # Reuse-check
//!
//! `rts-cranelift` owns nothing test-shaped. `crates/rts-std-rwk/src/test.rs`
//! already implements `describe`/`test`/`expect`, and [`record`]'s own doc
//! explains at length why this module keeps a second record rather than
//! reusing that one — read it before assuming this is duplication.
//!
//! # The execution model, and why it is simpler than the spec
//!
//! Real Node's runner is scheduled: `test()`'s returned `Promise` settles once
//! the body (possibly async) finishes, and `describe()`'s body only
//! *registers* children before they run. This module's `describe`/`test`
//! both run their body IMMEDIATELY and SYNCHRONOUSLY, the same shape
//! `rts-std-rwk::test`'s own `describe`/`test` already use — because nothing
//! in this crate can await a body (see "Not implemented, by name"), so
//! "register now, run later" would have nowhere later to run it.
//!
//! `before`/`after` are honored by scope: [`describe`] pushes a
//! [`Scope`] before running its body and pops it after, running that scope's
//! `after` hooks as it pops; `before` hooks in a scope run lazily, the first
//! time a `test`/`it` inside that scope runs. `beforeEach`/`afterEach` run
//! around every test, from the outermost enclosing scope to the innermost.
//!
//! # What a failure IS here
//!
//! This module cannot catch a thrown error: an entry point has no protected
//! region to unwind into (`crate::assert`, `rts-std-rwk::test`'s own module
//! doc). So the only two ways a test is recorded as failing are: its body
//! answers a value with a `.then` property (see [`refuse_if_async`]), or a
//! future assertion helper explicitly calls back into this module's failure
//! path — none exists yet. **A test whose body throws is not caught here and
//! its outcome is not defined by this module** — stated rather than silently
//! reported as a pass, per the honesty floor.
//!
//! # Refused, by name (the async forms named in the task)
//!
//! `await test(...)` does not compile at all — this engine's emitter does
//! not compile `await` in the positions that would need to appear here, so
//! there is no runtime path to refuse. A test function that itself returns a
//! thenable (`async (t) => {...}`, or a plain function returning a
//! `Promise`) DOES reach this module at run time; [`refuse_if_async`] detects
//! it by the presence of a `.then` member and records a failure naming the
//! reason, rather than reporting a pass for a body that has not actually
//! finished running.
//!
//! # Not implemented, by name
//!
//! - **Exception-based failure** — see "What a failure IS here" above.
//! - **`mock`** (`mock.fn`/`.method`/`.module`/`.property`/`.timers`) — every
//!   form hands back a wrapped, individually-identifiable callable. A native
//!   callable here is one fixed function pointer with no per-instance
//!   environment slot (`node:events`' own doc names the identical gap for
//!   `rawListeners`' invocable wrapper) — there is nowhere to keep which mock
//!   a given call belongs to, so no `mock.*` member is installed at all
//!   rather than one that would silently answer the wrong mock's history.
//! - **`snapshot`**, **coverage** (`--experimental-test-coverage`), **watch
//!   mode**, **process isolation** (`isolation: 'process'`, `run()`'s
//!   `TestsStream`), **reporters** (`node:test/reporters`) — none of these
//!   are reached; only `test`/`describe`/`it`/the four hooks are installed.
//! - **`TestContext`/`SuiteContext`** as documented objects — the body
//!   receives `undefined`, not a context with `.plan`/`.skip`/`.todo`/
//!   `.diagnostic`/`.mock`/`.waitFor`/`.test` (nested subtests). A body
//!   expecting its first parameter is refused nothing at the call site (it
//!   simply reads `undefined`), which is the same "reads as absent" contract
//!   every entry point in this crate already has for a missing member.
//! - **`only`/`skip`/`todo`/`expectFailure`**, name-pattern filtering,
//!   `concurrency`, `timeout`, `signal` — no `options` argument is read at
//!   all; every `test`/`describe` runs, unconditionally, in declaration
//!   order.
//! - **The default export / `.skip`/`.todo`/`.only`/`.expectFailure`
//!   attached to `test`** — only the bare `test`/`it`/`describe` names are
//!   installed.
//! - **`assert.register`**, `node:test`'s own `assert` namespace.

mod record;

use rts_core_rwk::entry::{self, Context, Provided};
use std::cell::RefCell;

/// One `describe`/top-level scope's pending hooks.
#[derive(Default)]
struct Scope {
    before: Vec<u64>,
    before_ran: bool,
    after: Vec<u64>,
    before_each: Vec<u64>,
    after_each: Vec<u64>,
}

thread_local! {
    /// The scope stack. Never empty: index 0 is the file's own top level, so
    /// a `before`/`test` outside any `describe` still has somewhere to land.
    /// Thread-local for the reason `record.rs` documents.
    static SCOPES: RefCell<Vec<Scope>> = RefCell::new(vec![Scope::default()]);
}

/// The namespace `node:test` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("describe", describe),
        ("suite", describe),
        ("test", test),
        ("it", test),
        ("before", before),
        ("after", after),
        ("beforeEach", before_each),
        ("afterEach", after_each),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `describe(name, body)` / `suite(name, body)`.
extern "C" fn describe(_e: u64, _this: u64, _name: u64, body: u64, _a2: u64, _a3: u64) -> u64 {
    SCOPES.with(|held| held.borrow_mut().push(Scope::default()));
    let absent = entry::undefined_value();
    entry::call(body, absent, absent, absent, absent, absent);
    let popped = SCOPES.with(|held| held.borrow_mut().pop());
    if let Some(scope) = popped {
        for hook in scope.after {
            entry::call(hook, absent, absent, absent, absent, absent);
        }
    }
    absent
}

/// `test(name, body)` / `it(name, body)`.
extern "C" fn test(_e: u64, _this: u64, name: u64, body: u64, _a2: u64, _a3: u64) -> u64 {
    let title = entry::described(name).unwrap_or_default();
    let absent = entry::undefined_value();
    run_pending_before();
    for hook in each_hooks(|scope| scope.before_each.clone()) {
        entry::call(hook, absent, absent, absent, absent, absent);
    }
    let answered = entry::call(body, absent, absent, absent, absent, absent);
    for hook in each_hooks(|scope| scope.after_each.clone()).into_iter().rev() {
        entry::call(hook, absent, absent, absent, absent, absent);
    }
    let failure = refuse_if_async(answered);
    record::push(title, failure);
    absent
}

/// Runs every enclosing scope's `before` hooks that have not run yet, from
/// outermost to innermost — lazily, on the first test reached inside each.
fn run_pending_before() {
    let absent = entry::undefined_value();
    let pending: Vec<u64> = SCOPES.with(|held| {
        let mut scopes = held.borrow_mut();
        let mut collected = Vec::new();
        for scope in scopes.iter_mut() {
            if !scope.before_ran {
                collected.extend(scope.before.iter().copied());
                scope.before_ran = true;
            }
        }
        collected
    });
    for hook in pending {
        entry::call(hook, absent, absent, absent, absent, absent);
    }
}

/// Every scope's hooks selected by `pick`, outermost first.
fn each_hooks(pick: impl Fn(&Scope) -> Vec<u64>) -> Vec<u64> {
    SCOPES.with(|held| held.borrow().iter().flat_map(&pick).collect())
}

/// A value with a `.then` property is a thenable this module cannot await —
/// see the module doc's "Refused, by name". `None` is a pass.
fn refuse_if_async(answered: u64) -> Option<String> {
    let then = entry::with_runtime(|context| entry::get_member(context, answered, "then"));
    let absent = entry::undefined_value();
    if then != absent {
        Some(
            "node:test: this test's body returned a thenable (async function or Promise); \
             awaiting is not implemented — see readline/test_runner's module doc"
                .to_owned(),
        )
    } else {
        None
    }
}

/// `before(fn)` — appended to the current scope, run lazily before its first
/// test.
extern "C" fn before(_e: u64, _this: u64, body: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    SCOPES.with(|held| {
        if let Some(scope) = held.borrow_mut().last_mut() {
            scope.before.push(body);
        }
    });
    entry::undefined_value()
}

/// `after(fn)` — run once, when the enclosing `describe` (or the file, for
/// the top-level scope) finishes.
extern "C" fn after(_e: u64, _this: u64, body: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    SCOPES.with(|held| {
        if let Some(scope) = held.borrow_mut().last_mut() {
            scope.after.push(body);
        }
    });
    entry::undefined_value()
}

/// `beforeEach(fn)`.
extern "C" fn before_each(_e: u64, _this: u64, body: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    SCOPES.with(|held| {
        if let Some(scope) = held.borrow_mut().last_mut() {
            scope.before_each.push(body);
        }
    });
    entry::undefined_value()
}

/// `afterEach(fn)`.
extern "C" fn after_each(_e: u64, _this: u64, body: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    SCOPES.with(|held| {
        if let Some(scope) = held.borrow_mut().last_mut() {
            scope.after_each.push(body);
        }
    });
    entry::undefined_value()
}

/// Everything reported since the last [`reset`], in order — the accessor a
/// host reads, the same shape `rts-std-rwk::test::record` has for its own
/// (separate) record.
pub fn record() -> Vec<record::Reported> {
    record::record()
}

/// Empties the record.
pub fn reset() {
    record::reset()
}
