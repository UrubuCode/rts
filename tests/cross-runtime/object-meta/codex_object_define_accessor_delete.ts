// Cross-runtime: accessor property delete and redefinition.
const obj: any = {};
let value = 1;
Object.defineProperty(obj, "x", {
  get() { return value; },
  set(v) { value = v * 2; },
  enumerable: true,
  configurable: true
});

obj.x = 5;
console.log(obj.x + ":" + value + ":" + Object.keys(obj).join(","));
console.log(delete obj.x);
obj.x = 9;
console.log(obj.x + ":" + value + ":" + Object.keys(obj).join(","));
