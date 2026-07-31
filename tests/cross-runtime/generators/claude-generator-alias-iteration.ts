// Cross-runtime: a generator reached through an ALIAS (`const g = gg`) must keep
// its protocol on the ITERATION paths — spread and `for-of` — not only on
// `.next()`.
//
// Regression guard for issue #2042: `sigs` is keyed by the DECLARED name, so
// `sigs.get("g")` missed the alias and the call looked like an ordinary
// function; spread and for-of then silently produced NOTHING (`[...g()]` → ""
// while `[...gg()]` → "1,2"). `.next()` already followed the alias.
//
// This matters far beyond an explicit alias: the parser rewrites EVERY generator
// EXPRESSION to `__genexpr_N` + `const g = __genexpr_N`, so
// `const g = function*(){…}` — the form minified bundles use — hit exactly this.

function* eagerDecl() { yield* [1, 2]; }
const aliasEager = eagerDecl;
console.log("spreadViaAlias=" + [...aliasEager()].join(","));

function* twoValues() { yield 1; yield 2; }
const aliasTwo = twoValues;
let sum = 0;
for (const x of aliasTwo()) { sum = sum + x; }
console.log("forOfViaAlias=" + sum);
console.log("nextViaAlias=" + aliasTwo().next().value);

// generator EXPRESSION — becomes an alias in the parser
const exprEager = function* () { yield* [3, 4]; };
console.log("spreadExprEager=" + [...exprEager()].join(","));

const exprLazy = function* () { let i = 1; while (i <= 3) { yield i; i = i + 1; } };
console.log("spreadExprLazy=" + [...exprLazy()].join(","));
console.log("nextExprLazy=" + exprLazy().next().value);

let lazySum = 0;
for (const x of exprLazy()) { lazySum = lazySum + x; }
console.log("forOfExprLazy=" + lazySum);

// an alias chain still resolves
const aliasOfAlias = aliasEager;
console.log("aliasChain=" + [...aliasOfAlias()].join(","));

// ── non-regressions: the direct declaration ─────────────────────────────────
console.log("spreadDirect=" + [...eagerDecl()].join(","));
let direct = 0;
for (const x of twoValues()) { direct = direct + x; }
console.log("forOfDirect=" + direct);
