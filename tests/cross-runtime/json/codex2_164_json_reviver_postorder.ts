// Cross-runtime: the parse reviver walks children before their containers.
const seen: string[] = [];
const value = JSON.parse('{"a":[1,2],"b":3}', function (key, v) {
  seen.push(key);
  if (typeof v === "number") return v * 10;
  return v;
});
console.log(JSON.stringify(value));
console.log(seen.join(","));

