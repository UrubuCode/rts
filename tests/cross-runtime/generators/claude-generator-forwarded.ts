// Cross-runtime: a LAZY generator reached THROUGH another function keeps its
// whole protocol — first value, sent value, spread, for-of.
//
// Regression guard: it was double-boxed. Since #2042 the lazy ctor's call site
// boxes the handle as a TAG_OBJECT word, but the fixpoint that propagates the
// generator mark to a FORWARDER still typed the forwarder as the raw `Int64`
// handle — so the already-boxed word was boxed again and the handle died
// (`wrapper().next().value` read `undefined`).

function* inner(o) { const a = yield o.v; yield a * 2; }
function wrapper() { const o = { v: 5 }; return inner(o); }
const it = wrapper();
console.log("first=" + it.next().value);
console.log("sent=" + it.next(10).value);

function* noParam() { const a = yield 1; yield a + 1; }
function fwd() { return noParam(); }
const i2 = fwd();
console.log("noParamFirst=" + i2.next().value);
console.log("noParamSent=" + i2.next(9).value);

function* counter(n) { let i = 0; while (i < n) { yield i; i = i + 1; } }
function fwd3() { return counter(3); }
console.log("spread=" + [...fwd3()].join(","));

let sum = 0;
for (const x of fwd3()) { sum = sum + x; }
console.log("forOf=" + sum);

// two levels of forwarding
function fwdA() { return counter(2); }
function fwdB() { return fwdA(); }
console.log("twoLevels=" + [...fwdB()].join(","));

// each call gets its OWN iterator, not shared state
const a1 = fwd3();
const a2 = fwd3();
a1.next();
console.log("independent=" + a2.next().value);

// the full result object survives the forward
console.log("resultObject=" + JSON.stringify(fwd().next()));

// ── non-regressions: the direct call ────────────────────────────────────────
const d = noParam();
console.log("directFirst=" + d.next().value);
console.log("directSent=" + d.next(4).value);
console.log("directSpread=" + [...counter(2)].join(","));
