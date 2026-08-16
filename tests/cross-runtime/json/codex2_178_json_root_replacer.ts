// Cross-runtime: the replacer receives the root under the empty-string key.
const seen: string[] = [];
const out = JSON.stringify({ x: 1 }, (key, value) => {
  seen.push(key);
  if (key === "") return { wrapped: value };
  return value;
});
console.log(out);
console.log(seen.map((x) => x === "" ? "<root>" : x).join(","));

