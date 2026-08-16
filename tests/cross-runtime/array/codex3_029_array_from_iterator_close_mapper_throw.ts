// Cross-runtime: Array.from closes its source iterator when the mapper throws.
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
let caught = "";
try {
  Array.from(iterable, (v) => { if (v === 2) throw new Error("stop"); return v; });
} catch (e: any) { caught = e.message; }
console.log(caught);
console.log(seen.join(","));

