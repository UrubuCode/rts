// Cross-runtime: lazy iteration observes getters at pull time and closes after consumer failure.
const seen: string[] = [];
const source: any = {
  get 0() { seen.push("get0"); return "a"; },
  get 1() { seen.push("get1"); return "b"; },
  length: 2,
};
const iterable = {
  *[Symbol.iterator]() {
    try {
      for (let i = 0; i < source.length; i++) yield source[i];
    } finally {
      seen.push("close");
    }
  },
};
try {
  for (const value of iterable) {
    seen.push("value:" + value);
    if (value === "a") throw new Error("stop");
  }
} catch (e: any) { seen.push("catch:" + e.message); }
console.log(seen.join("|"));

