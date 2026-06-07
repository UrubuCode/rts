// Cross-runtime: replacer property list coerces Number/String wrapper entries.
const keys: any[] = [new String("b"), new Number(1), Symbol("ignored"), null, "a"];
const obj: any = { a: "A", b: "B", "1": "one", null: "nil" };
console.log(JSON.stringify(obj, keys));
console.log(JSON.stringify([{ a: 1, b: 2, "1": 3, null: 4 }], keys));
