// Cross-runtime: ordinary coercion falls back when the preferred hook returns an object.
const seen: string[] = [];
const value = {
  valueOf() { seen.push("valueOf"); return {}; },
  toString() { seen.push("toString"); return "17"; },
};
console.log(+value);
console.log(seen.join(","));
seen.length = 0;
console.log(String(value));
console.log(seen.join(","));

