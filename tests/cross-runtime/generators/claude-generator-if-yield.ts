// Cross-runtime: an `if` whose branches contain `yield`, including `yield` in
// VALUE position (`const v = yield x`).
//
// This produced a WRONG VALUE, silently. An `if` containing yield was ineligible
// for the state machine, so it fell back to the eager buffer — which rewrites
// every `yield X` into a push, including in value position. `const v = yield 1`
// then read back `undefined` instead of the sent value.
//
// The state machine already had the branch-into-states path; it was only used
// when the `if` contained a `return`. It now covers `yield` too.
//
// Still ineligible on purpose: `yield` in the TEST (`if (yield x)`) would need
// to suspend in the MIDDLE of evaluating the condition.

function* withBlock() { if (true) { const v = yield 1; yield v; } }
const ia = withBlock();
console.log("blockFirst=" + ia.next().value);
console.log("blockSent=" + ia.next(5).value);

function* thenBranch() { const v = yield 1; if (v > 0) yield v * 2; else yield 0; }
const ib = thenBranch();
ib.next();
console.log("thenTaken=" + ib.next(5).value);

function* elseBranch() { const v = yield 1; if (v > 0) yield v * 2; else yield -1; }
const ic = elseBranch();
ic.next();
console.log("elseTaken=" + ic.next(-3).value);

function* nested() {
  const x = yield 1;
  if (x > 10) { const y = yield x; yield y + 1; } else { yield 0; }
}
const ig = nested();
console.log("nested=" + ig.next().value + "," + ig.next(20).value + "," + ig.next(7).value);

function* insideLoop() { let i = 0; while (i < 3) { if (i % 2 === 0) yield i; i = i + 1; } }
console.log("insideLoop=" + [...insideLoop()].join(","));

function* elseIfChain() {
  const v = yield 1;
  if (v === 1) yield "um";
  else if (v === 2) yield "dois";
  else yield "outro";
}
const ie = elseIfChain();
ie.next();
console.log("elseIfChain=" + ie.next(2).value);

// ── non-regressions ─────────────────────────────────────────────────────────
function* falseBranch() { if (false) { yield 9; } yield 1; }
console.log("branchNotTaken=" + [...falseBranch()].join(","));

function* noBraces() { if (true) yield 1; else yield 2; yield 3; }
console.log("noBraces=" + [...noBraces()].join(","));

function* withReturn() { if (true) { return; } yield 1; }
console.log("withReturn=" + [...withReturn()].join(",") + "|");

function* noIf() { const a = yield 1; yield a * 3; }
const sd = noIf();
console.log("noIf=" + sd.next().value + "," + sd.next(4).value);
