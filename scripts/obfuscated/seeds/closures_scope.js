// Closures, per-iteration bindings, shadowing, IIFE chains.
const out = [];
const fns = [];
for (let i = 0; i < 4; i++) fns.push(() => i * 3);
out.push(fns.map((f) => f()).join(","));

function counter(start) {
  let n = start;
  return { inc: () => ++n, get: () => n };
}
const c = counter(10);
c.inc(); c.inc();
out.push(c.get());

let x = "outer";
{
  let x = "inner";
  out.push(x);
}
out.push(x);

out.push((function (a) { return (function (b) { return a + b; })(5); })(7));

const memo = (() => {
  const seen = {};
  return (k) => (seen[k] = (seen[k] || 0) + 1);
})();
memo("a"); memo("a"); memo("b");
out.push(memo("a") + ":" + memo("b"));

console.log(out.join("|"));
