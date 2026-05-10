// Cross-runtime: JSON.parse reviver transform and this binding.
const seen: string[] = [];
const obj = JSON.parse('{"a":1,"b":2}', function (this: any, key, value) {
  if (key === "b") seen.push(String(this.a));
  return typeof value === "number" ? value * 2 : value;
});
console.log("obj=" + JSON.stringify(obj));
console.log("seen=" + seen.join("|"));
