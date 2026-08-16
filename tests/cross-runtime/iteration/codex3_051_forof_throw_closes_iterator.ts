// Cross-runtime: throwing from a for-of body closes the iterator before propagation.
const seen: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    return {
      next() { seen.push("next"); return { value: 1, done: false }; },
      return() { seen.push("return"); return { done: true }; },
    };
  },
};
try {
  for (const value of iterable) {
    seen.push("body:" + value);
    throw new Error("boom");
  }
} catch (e: any) { seen.push("catch:" + e.message); }
console.log(seen.join("|"));

