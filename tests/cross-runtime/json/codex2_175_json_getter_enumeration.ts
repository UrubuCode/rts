// Cross-runtime: stringify reads enumerable getters in property order.
const seen: string[] = [];
const value = {
  get a() { seen.push("a"); return 1; },
  get b() { seen.push("b"); return { c: 2 }; },
};
console.log(JSON.stringify(value));
console.log(seen.join(","));

