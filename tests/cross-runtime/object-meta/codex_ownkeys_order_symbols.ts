// Cross-runtime: Reflect.ownKeys order with integer keys, strings, and symbols.
const s1 = Symbol("s1");
const s2 = Symbol("s2");
const obj: any = {};
obj.b = 1;
obj[2] = "two";
obj.a = 2;
obj[1] = "one";
obj[s1] = 3;
obj.c = 4;
obj[s2] = 5;

console.log(Reflect.ownKeys(obj).map(k => typeof k === "symbol" ? String(k) : k).join(","));
console.log(Object.getOwnPropertyNames(obj).join(","));
console.log(Object.getOwnPropertySymbols(obj).map(String).join(","));
