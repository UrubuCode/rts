// Cross-runtime: Reflect API methods.

// Reflect.has (like in operator)
const obj = { a: 1, b: 2 };
console.log(Reflect.has(obj, "a"));
console.log(Reflect.has(obj, "z"));

// Reflect.ownKeys
console.log(Reflect.ownKeys({ x: 1, y: 2 }).join(","));

// Reflect.get / Reflect.set
const target: any = { val: 10 };
console.log(Reflect.get(target, "val"));
Reflect.set(target, "val", 99);
console.log(target.val);

// Reflect.deleteProperty
Reflect.deleteProperty(target, "val");
console.log("val" in target);

// Reflect.apply
function add(a: number, b: number) { return a + b; }
console.log(Reflect.apply(add, null, [3, 4]));

// Reflect.construct
class Point { x: number; y: number; constructor(x: number, y: number) { this.x = x; this.y = y; } }
const p = Reflect.construct(Point, [1, 2]);
console.log(p.x, p.y);
console.log(p instanceof Point);

// Reflect.defineProperty
const o: any = {};
Reflect.defineProperty(o, "id", { value: 42, writable: false, enumerable: true });
console.log(o.id);

// Reflect.getPrototypeOf
console.log(Reflect.getPrototypeOf([]) === Array.prototype);
