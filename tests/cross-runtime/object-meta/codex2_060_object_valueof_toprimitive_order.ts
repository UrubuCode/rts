// Cross-runtime: numeric and string hints choose coercion hooks in different orders.
const seen: string[] = [];
const o = {
  valueOf() { seen.push("valueOf"); return 10; },
  toString() { seen.push("toString"); return "S"; },
};
console.log(+o, seen.join(","));
seen.length = 0;
console.log(String(o), seen.join(","));

