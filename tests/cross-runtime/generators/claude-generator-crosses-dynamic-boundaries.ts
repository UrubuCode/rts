// Cross-runtime: a generator's iterator must keep the `next`/`return`/`throw`
// protocol after crossing a DYNAMIC boundary — array, object field, argument,
// class field, Map value, destructuring.
//
// Regression guard for issue #2042: the lazy-generator ctor hands back the
// `Entry::GenState` handle in a raw `Repr::Int64`, and the engine stamped every
// `Int64` as a NUMBER — so boxing the value to cross a boundary converted the
// handle to a double and the identity was lost. `.next()` then read `undefined`
// silently instead of throwing.
function* gen() {
  let i = 1;
  while (i <= 3) {
    yield i;
    i = i + 1;
  }
}

// ── the boundaries ──────────────────────────────────────────────────────────
console.log("array=" + [gen()][0].next().value);

const obj = { it: gen() };
console.log("objectField=" + obj.it.next().value);

function consume(x) {
  return x.next();
}
console.log("argument=" + consume(gen()).value);

const pushed: any[] = [];
pushed.push(gen());
console.log("push=" + pushed[0].next().value);

class Holder {
  it: any;
  constructor() {
    this.it = gen();
  }
}
console.log("classField=" + new Holder().it.next().value);

const map = new Map();
map.set("k", gen());
console.log("mapValue=" + map.get("k").next().value);

const [destructured] = [gen()];
console.log("destructuring=" + destructured.next().value);

// The stored iterator must be the SAME one, so the cursor advances across calls
// (a copy would restart at 1 every time and print 3).
const kept = { it: gen() };
console.log(
  "cursorAdvances=" +
    (kept.it.next().value + kept.it.next().value + kept.it.next().value),
);

// The full result object survives the boundary, not just `.value`.
console.log("resultObject=" + JSON.stringify([gen()][0].next()));

// Exhaustion still reports `done` correctly after the boundary.
const drained = { it: gen() };
drained.it.next();
drained.it.next();
drained.it.next();
console.log("afterExhaustion=" + JSON.stringify(drained.it.next()));

// `return()` through the boundary closes the iterator.
const closed = { it: gen() };
console.log("returnMethod=" + JSON.stringify(closed.it.return(99)));
console.log("afterReturn=" + JSON.stringify(closed.it.next()));

// ── non-regressions: the value model must not over-apply the fix ────────────
console.log("direct=" + gen().next().value);
console.log("intInArray=" + [42][0]);
console.log("bigIntInArray=" + [9007199254740991][0]);
console.log("plainArrayNext=" + ([1, 2] as any).next);
console.log("spread=" + [...gen()].join(","));

let sum = 0;
for (const x of gen()) {
  sum = sum + x;
}
console.log("forOf=" + sum);
