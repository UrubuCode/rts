// Cross-runtime: partial array destructuring closes a custom iterator.
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
const [first, second] = iterable;
console.log(first, second);
console.log(seen.join(","));

