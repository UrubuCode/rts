// Cross-runtime: a LAZY generator (state-machine — a loop containing `yield`)
// whose parameter carries NO type annotation must still yield its values.
//
// Regression guard for issue #2044. `sig.rs` widens a function's RETURN to
// `Tagged` when any parameter is `Tagged` — and an unannotated parameter is
// exactly that. But a lazy generator's ctor does not return a value: it returns
// the opaque `Entry::GenState` HANDLE, which must stay `Int64`. Widened, the
// return was coerced with `fcvt_from_sint`, the handle became an ordinary double
// and the iterator silently yielded nothing (`[...g(1)]` → `[]`).
//
// This is the form that shows up in real code: bundled JS is minified and has no
// type annotations, so the annotated variant was the only one being exercised.

function* untyped(start, count) {
  for (let i = 0; i < count; i++) yield start + i;
}
function* annotated(start: number, count: number) {
  for (let i = 0; i < count; i++) yield start + i;
}
function* noParams() {
  let n = 1;
  while (n <= 3) {
    yield n;
    n = n + 1;
  }
}
function* withDefault(start = 1) {
  let n = start;
  while (n < start + 3) {
    yield n;
    n = n + 1;
  }
}
function* stringParam(p) {
  let i = 0;
  while (i < 2) {
    yield p + i;
    i = i + 1;
  }
}
// A linear body stays on the EAGER path — it must keep working too.
function* eagerParam(s) {
  yield s;
  yield s + 1;
}

console.log("spread=" + [...untyped(10, 3)].join(","));
console.log("next=" + untyped(10, 3).next().value);

let sum = 0;
for (const x of untyped(1, 3)) {
  sum = sum + x;
}
console.log("forOf=" + sum);

// crossing a dynamic boundary as well (issue #2042)
console.log("acrossBoundary=" + [untyped(10, 3)][0].next().value);

// the cursor must advance — the same iterator, not a fresh copy each call
const shared = untyped(100, 3);
console.log(
  "cursorAdvances=" +
    (shared.next().value + shared.next().value + shared.next().value),
);

// the full result object, and exhaustion
console.log("resultObject=" + JSON.stringify(untyped(10, 1).next()));
const drained = untyped(10, 1);
drained.next();
console.log("afterExhaustion=" + JSON.stringify(drained.next()));

// an INFINITE generator is only expressible on the lazy path (the eager buffer
// would materialize forever) — it must yield lazily, on demand.
function* naturals(start = 1) {
  let n = start;
  while (true) {
    yield n;
    n = n + 1;
  }
}
function take(iter, n) {
  const out: any[] = [];
  for (let i = 0; i < n; i++) {
    const r = iter.next();
    if (r.done) break;
    out.push(r.value);
  }
  return out;
}
console.log("takeInfinite=" + take(naturals(), 5).join(","));
console.log("takeInfiniteFrom=" + take(naturals(10), 4).join(","));

// ── non-regressions: the other generator paths ──────────────────────────────
console.log("annotated=" + [...annotated(10, 3)].join(","));
console.log("noParams=" + [...noParams()].join(","));
console.log("defaultOmitted=" + [...withDefault()].join(","));
console.log("defaultPassed=" + [...withDefault(5)].join(","));
console.log("stringParam=" + [...stringParam("x")].join(","));
console.log("eagerParam=" + [...eagerParam(7)].join(","));
