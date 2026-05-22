// Cross-runtime: Object.defineProperty
const obj = {};
Object.defineProperty(obj, "a", {
  value: 42,
  writable: false,
  enumerable: true,
  configurable: false
});

console.log("value=" + obj.a);
obj.a = 100;
console.log("still=" + obj.a);

Object.defineProperty(obj, "b", {
  value: 10,
  enumerable: false
});

console.log("keys=" + Object.keys(obj).join(","));
console.log("b=" + obj.b);

// Getter/setter
Object.defineProperty(obj, "c", {
  get() { return this._c || 0; },
  set(val) { this._c = val * 2; },
  enumerable: true
});

obj.c = 5;
console.log("getter=" + obj.c);
