// Cross-runtime: continue does not close an iterator before normal exhaustion.
const seen: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let n = 0;
    return {
      next() { n++; seen.push("next:" + n); return { value: n, done: n > 3 }; },
      return() { seen.push("return"); return { done: true }; },
    };
  },
};
for (const x of iterable) {
  if (x === 2) continue;
  seen.push("value:" + x);
}
console.log(seen.join("|"));

