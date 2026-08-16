// Cross-runtime: Object.assign reads source getters in enumeration order.
const seen: string[] = [];
const source = {
  get a() { seen.push("a"); return 1; },
  get b() { seen.push("b"); return 2; },
};
const target = Object.assign({ z: 0 }, source);
console.log(JSON.stringify(target));
console.log(seen.join(","));

