// Cross-runtime: mutating object keys does not affect Map identity.
const key: any = { id: 1 };
const m = new Map<any, string>([[key, "old"]]);
key.id = 2;
const sameShape = { id: 2 };
m.set(sameShape, "new");

console.log(m.get(key));
console.log(m.get({ id: 2 }));
console.log(m.size);
console.log([...m.keys()].map(k => k.id).join(","));
