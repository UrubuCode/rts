// Cross-runtime: breaking for-of closes a custom iterator with return.
const seen: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let n = 0;
    return {
      next() { seen.push("next"); return { value: ++n, done: false }; },
      return() { seen.push("return"); return { done: true }; },
    };
  },
};
for (const x of iterable) {
  seen.push("value:" + x);
  break;
}
console.log(seen.join(","));

