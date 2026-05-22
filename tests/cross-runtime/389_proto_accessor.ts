// Cross-runtime: __proto__ accessor vs Object.setPrototypeOf vs Object.create.

// __proto__ is an accessor on Object.prototype (deprecated but widely supported)
const base = { type: "base", greet() { return "hi from " + this.type; } };
const obj: any = { type: "child" };
obj.__proto__ = base;
console.log(obj.greet());                             // hi from child
console.log(Object.getPrototypeOf(obj) === base);    // true

// __proto__ in object literal sets prototype
const proto = { hello() { return "hello"; } };
const withProto: any = { __proto__: proto, world() { return "world"; } };
console.log(withProto.hello());  // hello
console.log(withProto.world());  // world
console.log(Object.getPrototypeOf(withProto) === proto); // true

// Object.setPrototypeOf is the official way
const src = { x: 1 };
const newProto = { double() { return (this as any).x * 2; } };
Object.setPrototypeOf(src, newProto);
console.log((src as any).double());  // 2

// __proto__ = null removes prototype entirely
const noProto: any = { a: 1 };
noProto.__proto__ = null;
console.log(Object.getPrototypeOf(noProto));  // null
console.log("toString" in noProto);           // false

// __proto__ property key vs __proto__ setter (tricky edge case)
// Using Object.create(null) prevents __proto__ from being a setter
const safe = Object.create(null) as any;
safe.__proto__ = { injected: true };  // just a regular property, not setting prototype
console.log(Object.getPrototypeOf(safe));  // null (not changed)
console.log(safe.__proto__.injected);      // true (it's just a property)

// Prototype chain length measurement
function protoDepth(obj: any): number {
  let depth = 0;
  let p = Object.getPrototypeOf(obj);
  while (p !== null) { depth++; p = Object.getPrototypeOf(p); }
  return depth;
}
console.log(protoDepth({}));            // 1 (Object.prototype)
console.log(protoDepth([]));            // 2 (Array.prototype -> Object.prototype)
console.log(protoDepth(new Map()));     // 2
console.log(protoDepth(Object.create(null))); // 0
