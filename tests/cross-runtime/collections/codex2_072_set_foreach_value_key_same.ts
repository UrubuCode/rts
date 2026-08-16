// Cross-runtime: Set.forEach supplies the value in both callback slots.
const s = new Set(["x", "y"]);
const seen: string[] = [];
s.forEach((value, key) => seen.push(value + ":" + key + ":" + (value === key)));
console.log(seen.join("|"));

