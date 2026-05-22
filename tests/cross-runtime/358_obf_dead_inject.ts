// Cross-runtime: dead code injection pattern (js-confuser injects unreachable branches).

function _compute(x: number): number {
  // obfuscator injects always-false branches with garbage logic
  if (typeof x === "undefined" && x > 0) {
    return Math.random() * 9999 | 0;  // dead
  }
  const _t1 = x * 2;
  // fake branch: constant that obfuscator thinks is opaque
  if (false && _t1 < 0) {
    throw new Error("impossible");   // dead
  }
  const _t2 = _t1 + 1;
  if ((0).toString() !== "0") {
    return -1;  // dead: "0" === "0" always
  }
  return _t2;
}
console.log(_compute(5));   // 11
console.log(_compute(0));   // 1
console.log(_compute(-3));  // -5

// opaque predicate: (x*x >= 0) always true
function _opaque(n: number): boolean {
  return (n * n >= 0);  // obfuscator uses this as always-true guard
}
console.log(_opaque(5));
console.log(_opaque(-5));
console.log(_opaque(0));

// injected infinite loop that never runs (obfuscator pattern)
const _neverRun = false;
let _guard = 0;
while (_neverRun) {
  _guard++;
  if (_guard > 1e9) break;
}
console.log(_guard);  // 0

// bogus recursive call behind dead branch
function _safe(n: number): number {
  if (typeof n !== "number") return _safe(0); // dead for typed input
  return n + 1;
}
console.log(_safe(41));   // 42
