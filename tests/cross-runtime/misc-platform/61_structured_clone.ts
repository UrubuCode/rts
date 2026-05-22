// Cross-runtime compatibility: structuredClone with nested data.
const original = {
  id: 7,
  nested: { ok: true, list: [1, 2, 3] },
  map: new Map([
    ["a", 1],
    ["b", 2],
  ]),
  set: new Set(["x", "y"]),
};

const copy = structuredClone(original);
copy.nested.list.push(4);
copy.map.set("c", 3);
copy.set.add("z");

console.log("orig_list=" + original.nested.list.join(","));
console.log("copy_list=" + copy.nested.list.join(","));
console.log("orig_map=" + Array.from(original.map.entries()).map(([k, v]) => k + ":" + v).join(","));
console.log("copy_map=" + Array.from(copy.map.entries()).map(([k, v]) => k + ":" + v).join(","));
console.log("orig_set=" + Array.from(original.set.values()).join(","));
console.log("copy_set=" + Array.from(copy.set.values()).join(","));
