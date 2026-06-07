// Cross-runtime: JSON.parse reviver can delete properties and replace root.
const log: string[] = [];
const value = JSON.parse('{"a":1,"b":{"c":2},"d":[3,4]}', function (k, v) {
  log.push(k + ":" + (Array.isArray(v) ? "array" : typeof v));
  if (k === "c") return undefined;
  if (k === "") return { root: v };
  return v;
});

console.log(JSON.stringify(value));
console.log(log.join("|"));
