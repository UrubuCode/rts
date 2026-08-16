// ONE thing: a `next` that answers something which is not an Object. The spec
// demands a TypeError right there, BEFORE `done` is read — an engine that reads
// `done` off a primitive gets `undefined`, treats it as falsy, and loops
// forever. That is why this file stands alone, and why every consumer below is
// also bounded by a hard counter: a broken engine stops at the guard instead of
// running until the harness kills it.
let pulls = 0;
function makeBad(result: any): any {
  return {
    [Symbol.iterator]() {
      return { next() { if (++pulls > 500) return { value: undefined, done: true }; return result; } };
    },
  };
}

function probe(label: string, f: () => any) {
  pulls = 0;
  try { console.log(label + "=ok:" + String(f()) + " pulls=" + pulls); }
  catch (e: any) { console.log(label + "=" + e.constructor.name + " pulls=" + pulls); }
}

// Every primitive kind a `next` might wrongly answer.
for (const bad of [5, "s", true, null, undefined, 0, ""]) {
  const label = "next_" + (bad === null ? "null" : bad === undefined ? "undefined" : typeof bad + "_" + JSON.stringify(bad));
  probe(label + "_spread", () => [...makeBad(bad)].length);
}
probe("next_symbol_spread", () => [...makeBad(Symbol("s"))].length);
probe("next_bigint_spread", () => [...makeBad(1n)].length);

// The same refusal must reach every consumer, not just spread.
probe("forOf", () => { let n = 0; for (const _v of makeBad(5)) n++; return n; });
probe("arrayFrom", () => Array.from(makeBad(5)).length);
probe("newSet", () => new Set(makeBad(5)).size);
probe("newMap", () => new Map(makeBad(5)).size);
// Promise.all answers a REJECTED promise rather than throwing synchronously,
// so the rejection is handled here — an unhandled one changes the exit code.
Promise.all(makeBad(5)).then(
  () => console.log("promiseAll=resolved"),
  (e: any) => console.log("promiseAll=" + e.constructor.name),
);
probe("destructure", () => { const [a] = makeBad(5); return String(a); });
probe("yieldStar", () => { function* g() { yield* makeBad(5); } return [...g()].length; });
probe("objectSpreadIsNotIterable", () => JSON.stringify({ ...makeBad(5) }));

// A `next` that is not callable, and an absent `next`.
const noNext: any = { [Symbol.iterator]() { return {}; } };
probe("missingNext", () => [...noNext].length);
const badNextType: any = { [Symbol.iterator]() { return { next: 42 }; } };
probe("nonCallableNext", () => [...badNextType].length);

// A `Symbol.iterator` that answers a primitive is refused before any pull.
const primIter: any = { [Symbol.iterator]() { return 7; } };
probe("primitiveIterator", () => [...primIter].length);
const nullIter: any = { [Symbol.iterator]() { return null; } };
probe("nullIterator", () => [...nullIter].length);

// A WELL-FORMED result object with a missing `done` is legal: undefined is
// falsy, so it must be treated as not-done — bounded here by the value itself.
let n2 = 0;
const noDone: any = { [Symbol.iterator]() { return { next() { return { value: n2++ } as any; } }; } };
const collected: number[] = [];
for (const v of noDone) { collected.push(v as number); if (collected.length >= 3) break; }
console.log("missingDone=" + collected.join(",") + " (undefined is falsy, so not done)");

// A result object with a `done` GETTER, read exactly once per pull.
//
// Pulled MANUALLY rather than with spread. The bound has to live on the
// consumer side: an engine that never calls the getter reads `done` as
// undefined, and a guard placed inside the getter would never run either.
let doneReads = 0;
let i3 = 0;
const getterIt: any = { next() { return { value: i3++, get done() { doneReads++; return i3 > 3; } }; } };
const gd: number[] = [];
for (let k = 0; k < 20; k++) {
  const r = getterIt.next();
  if (r.done) break;
  gd.push(r.value);
}
console.log("getterDone=" + gd.join(",") + " reads=" + doneReads + " bounded=" + (gd.length < 20));

// A `done` getter that throws propagates out of the consumer. Bounded on the
// consumer side for the same reason as above — an engine that never reads the
// getter neither throws nor stops.
let throwPulls = 0;
const throwingIt: any = { next() { throwPulls++; return { value: 1, get done(): boolean { throw new RangeError("boom"); } }; } };
try {
  for (let k = 0; k < 20; k++) { const r = throwingIt.next(); if (r.done) break; }
  console.log("throwingDone=no-throw pulls=" + throwPulls);
} catch (e: any) {
  console.log("throwingDone=" + e.constructor.name + " pulls=" + throwPulls);
}

// A `value` getter is read only when the consumer wants the value, and never
// after done is true. `done` is a plain data property here, so this one is
// bounded by construction.
let valueReads = 0;
let i4 = 0;
const lazyValue: any = {
  [Symbol.iterator]() {
    return { next() { const d = i4 >= 2; i4++; return { get value() { valueReads++; return i4; }, done: d }; } };
  },
};
console.log("lazyValue=" + [...lazyValue].length + " valueReads=" + valueReads);
