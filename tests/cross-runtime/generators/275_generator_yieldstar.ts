// Cross-runtime: generator yield* delegation and return value.
function* inner() { yield 1; yield 2; return 9; }
function* outer() { const r = yield* inner(); yield r + 1; return r + 2; }
const g = outer();
const out = [g.next(), g.next(), g.next(), g.next()].map((x) => String(x.value) + ":" + x.done);
console.log("steps=" + out.join("|"));
