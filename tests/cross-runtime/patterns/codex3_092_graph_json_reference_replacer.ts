// Cross-runtime: a graph serializer combines WeakMap identity, JSON replacer order, and arrays.
const root: any = { name: "root" };
root.left = { name: "child", parent: root };
root.right = root.left;
const ids = new WeakMap<object, number>();
let next = 1;
const encoded = JSON.stringify(root, function (key, value) {
  if (value && typeof value === "object") {
    if (ids.has(value)) return { $ref: ids.get(value) };
    ids.set(value, next++);
  }
  return value;
});
console.log(encoded);
console.log(next);

