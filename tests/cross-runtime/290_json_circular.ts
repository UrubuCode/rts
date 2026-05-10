// Cross-runtime: JSON.stringify circular reference throws.
const obj: any = { a: 1 };
obj.self = obj;
let out = "";
try { JSON.stringify(obj); } catch (e: any) { out = e.name; }
console.log("err=" + out);
