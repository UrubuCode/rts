// Cross-runtime: Reflect.defineProperty and getOwnPropertyDescriptor
const obj = {};

const success = Reflect.defineProperty(obj, "a", {
  value: 42,
  writable: false,
  enumerable: true
});

console.log("success=" + success);
console.log("a=" + (obj as any).a);

const desc = Reflect.getOwnPropertyDescriptor(obj, "a");
console.log("value=" + desc?.value);
console.log("writable=" + desc?.writable);
console.log("enumerable=" + desc?.enumerable);

// Non-existent
const desc2 = Reflect.getOwnPropertyDescriptor(obj, "b");
console.log("desc2=" + desc2);
