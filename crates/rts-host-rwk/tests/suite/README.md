# The conformance fixtures

Each `.js` file here is a program that checks itself and answers **the names of
what failed**, as a comma-separated string. An empty answer is a pass.

The shape every fixture uses:

    let failed = "";
    function check(name, held) { if (!held) { failed = failed + name + ","; } }

    check("some-behaviour", 1 + 1 === 2);

    return failed;

`check` takes the name first so a reader scanning the file sees what is being
claimed before the expression that claims it — and so a failure report is a list
of names rather than a list of line numbers.

## What a fixture may not use

The emitter refuses these by name, and a fixture is subject to all of them:
`async`/`await`, generators, destructuring anywhere (including `const {a} = o`
and `for (const [k, v] of m)`), optional chaining, default parameters, a spread
in an object literal, `this` inside an arrow, `using`, and any function of more
than four parameters. The host wraps the source in a function, so a fixture
`return`s rather than exporting.

A fixture that does not COMPILE is a failure, not a skip. `suite.rs` reports the
refusal by name, which is the diagnostic — a suite that skipped what it could
not build would publish a number about the subset it happened to like, which is
the failure the honesty floor names.

## Why this is beside `running.rs` rather than instead of it

`running.rs` asserts from Rust, and that is the right shape for a behaviour
whose *encoding* matters: a boolean has to come back tagged as one, and only
Rust can check that. These fixtures are for coverage, where a hundred assertions
about `Array` semantics do not each want a Rust function around them.
