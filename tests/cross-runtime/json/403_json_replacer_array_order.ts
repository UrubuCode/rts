// Cross-runtime: JSON.stringify replacer array ordering and duplicate keys.
const obj: any = { b: 2, a: 1, c: 3, nested: { z: 9, a: 8 } };
const keys: any[] = ["c", "a", "missing", "a", new String("nested")];

console.log(JSON.stringify(obj, keys));
console.log(JSON.stringify([obj, { a: 4, c: 5 }], keys));
console.log(JSON.stringify({ "1": "one", "01": "zero-one", a: "aye" }, [1, "01", "a"]));
