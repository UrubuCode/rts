// Generators, yield*, iterator protocol driven by hand, async ordering.
const out = [];
function* inner() { yield "i1"; yield "i2"; return "iend"; }
function* outer() { out.push("pre"); const r = yield* inner(); out.push("got:" + r); yield "o1"; }
out.push([...outer()].join(","));
const it = inner();
out.push(JSON.stringify(it.next()), JSON.stringify(it.next()));
out.push(JSON.stringify(it.next()), JSON.stringify(it.next()));
function* counter() { let n = 0; while (true) { const step = yield n; n += step === undefined ? 1 : step; if (n > 6) return n; } }
const c = counter();
out.push(c.next().value, c.next(3).value, c.next(3).value);
function* withFinally() { try { yield 1; yield 2; } finally { out.push("genfin"); } }
const g = withFinally();
g.next();
out.push(JSON.stringify(g.return("bye")));
const manual = { [Symbol.iterator]() { let i = 0; return { next: () => (i < 3 ? { value: i++, done: false } : { value: undefined, done: true }) }; } };
out.push([...manual].join("+"));
for (const v of manual) { if (v === 1) break; out.push("m" + v); }
console.log(out.join("|"));
