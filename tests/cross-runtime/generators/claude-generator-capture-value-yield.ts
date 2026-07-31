// Cross-runtime: `yield` in VALUE position (`const a = yield x`) inside a
// generator expression that CAPTURES the enclosing scope.
//
// It looked like a contradiction: a value-yield needs the state machine, the
// state machine needs the generator hoisted to the top level, and hoisting loses
// the captures. The way out is to hoist with the CAPTURES AS PARAMETERS and
// leave an ordinary wrapper behind that forwards them — the body already refers
// to the captures by those names, so nothing is renamed.
//
// This also guards a WRONG VALUE: the previous attempt decided whether the case
// was supported by checking for a leftover `yield` AFTER desugaring, but the
// eager buffer rewrites every `yield X` into a push — including in value
// position — so `const a = yield o.v` silently became `const a = push(...)` and
// produced `5,NaN` instead of `5,20`.

function withObject() {
  const o = { v: 5 };
  const g = function* () { const a = yield o.v; yield a * 2; };
  const it = g();
  return it.next().value + "," + it.next(10).value;
}
console.log("captureObject=" + withObject());

function withFnAndConst() {
  const mult = 3;
  function h(x) { return x + 1; }
  const g = function* () { const a = yield h(1); yield a * mult; };
  const it = g();
  return it.next().value + "," + it.next(4).value;
}
console.log("captureFnAndConst=" + withFnAndConst());

function withParam(p) {
  const g = function* () { const a = yield p; yield a + p; };
  const it = g();
  return it.next().value + "," + it.next(10).value;
}
console.log("captureParam=" + withParam(2));

// each call captures its OWN scope
function counterFrom(start) {
  const g = function* () { const a = yield start; yield a + start; };
  const it = g();
  return it.next().value + "," + it.next(100).value;
}
console.log("independentA=" + counterFrom(1));
console.log("independentB=" + counterFrom(50));

// ── non-regressions ─────────────────────────────────────────────────────────
function captureNoValueYield() {
  const base = 10;
  const g = function* () { yield base; yield base + 1; };
  return [...g()].join(",");
}
console.log("captureStatementYield=" + captureNoValueYield());

const atTop = function* () { const a = yield 1; yield a + 1; };
const t = atTop();
console.log("topLevelValueYield=" + t.next().value + "," + t.next(5).value);

function noCapture() {
  const g = function* () { yield 42; };
  return g().next().value;
}
console.log("nestedNoCapture=" + noCapture());

function* declared() { const a = yield 1; yield a * 3; }
const d = declared();
console.log("declaration=" + d.next().value + "," + d.next(4).value);
