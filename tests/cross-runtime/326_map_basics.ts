// Cross-runtime: Map basic operations.
const m = new Map<string, number>();
m.set("a", 1);
m.set("b", 2);
m.set("c", 3);

console.log(m.size);
console.log(m.get("a"));
console.log(m.get("z"));
console.log(m.has("b"));
console.log(m.has("z"));

m.delete("b");
console.log(m.size);
console.log(m.has("b"));

const keys: string[] = [];
const vals: number[] = [];
m.forEach((v, k) => { keys.push(k); vals.push(v); });
console.log(keys.join(","));
console.log(vals.join(","));

m.clear();
console.log(m.size);
