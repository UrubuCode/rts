// Cross-runtime: a nested generator EXPRESSION must keep the variables it
// CAPTURES from the enclosing scope.
//
// Regression guard for the WhatsApp-Web bundle campaign (#2038): the parser
// hoisted EVERY generator expression to a top-level `__genexpr_N`, where the
// captured names no longer exist — `ReferenceError: o is not defined` in the
// engine, `call to unknown function 'o'` in the bundles.
//
// At the TOP level the free names already ARE globals, so hoisting stays safe
// there; inside a block, a capturing generator is desugared IN PLACE and stops
// being a generator, so the ordinary closure machinery carries the captures.

function withCapture() {
  const o = { v: 5 };
  function r(x) { return x + 1; }
  const g = function* () { yield r(o.v); };
  return g().next().value;
}
console.log("next=" + withCapture());

function captureSpread() {
  const base = 10;
  const g = function* () { yield base; yield base + 1; };
  return [...g()].join(",");
}
console.log("spread=" + captureSpread());

function captureForOf() {
  const arr = [1, 2, 3];
  const g = function* () { for (const x of arr) yield x * 2; };
  let s = 0;
  for (const v of g()) { s = s + v; }
  return s;
}
console.log("forOf=" + captureForOf());

function captureParam(mult) {
  const g = function* () { yield 1 * mult; yield 2 * mult; };
  return [...g()].join(",");
}
console.log("param=" + captureParam(3));

function captureTwoLevels() {
  const a = 100;
  function middle() {
    const b = 20;
    const g = function* () { yield a + b; };
    return g().next().value;
  }
  return middle();
}
console.log("twoLevels=" + captureTwoLevels());

// Each call gets its OWN capture — the generator is not shared state.
function makeCounter(start) {
  const g = function* () { yield start; yield start + 1; };
  return [...g()].join(",");
}
console.log("independentA=" + makeCounter(1));
console.log("independentB=" + makeCounter(50));

// ── non-regressions ─────────────────────────────────────────────────────────
const atTop = function* () { yield 7; };
console.log("topLevel=" + atTop().next().value);

function noCapture() {
  const g = function* () { yield 42; };
  return g().next().value;
}
console.log("nestedNoCapture=" + noCapture());

function* declared() { yield 1; yield 2; }
console.log("declaration=" + [...declared()].join(","));
