// Cross-runtime: the reviver may replace the parsed root value.
const seen: string[] = [];
const out = JSON.parse('{"x":2}', (key, value) => {
  seen.push(key);
  return key === "" ? value.x * 5 : value;
});
console.log(out);
console.log(seen.map((x) => x === "" ? "<root>" : x).join(","));

