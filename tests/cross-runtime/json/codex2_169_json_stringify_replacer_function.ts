// Cross-runtime: a replacer function transforms values with owning-object this.
const value = { a: 1, nested: { b: 2 } };
const seen: string[] = [];
const out = JSON.stringify(value, function (key, v) {
  seen.push(key + ":" + (this === value ? "root" : "other"));
  return typeof v === "number" ? v + 10 : v;
});
console.log(out);
console.log(seen.join("|"));

