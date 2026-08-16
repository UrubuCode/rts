// Cross-runtime: a throwing destructuring default closes the still-open iterator.
const seen: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let n = 0;
    return {
      next() { n++; seen.push("next:" + n); return { value: undefined, done: false }; },
      return() { seen.push("return"); return { done: true }; },
    };
  },
};
try {
  const [x = (() => { throw new Error("default"); })()] = iterable;
  void x;
} catch (e: any) { seen.push("catch:" + e.message); }
console.log(seen.join("|"));

