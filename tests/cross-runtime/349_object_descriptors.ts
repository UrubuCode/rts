// Cross-runtime: property descriptors — writable, enumerable, configurable.
const obj: any = {};

Object.defineProperty(obj, "readonly", { value: 42, writable: false, enumerable: true, configurable: false });
Object.defineProperty(obj, "hidden", { value: 99, writable: true, enumerable: false, configurable: true });
Object.defineProperty(obj, "normal", { value: 1, writable: true, enumerable: true, configurable: true });

console.log(obj.readonly);
console.log(obj.hidden);

// writable: false — throws in strict mode, silent fail in sloppy
try { obj.readonly = 100; } catch (_) {}
console.log(obj.readonly);  // still 42

// enumerable: false — does not appear in for..in or Object.keys
console.log(Object.keys(obj).join(","));  // only normal, readonly

// configurable: false — cannot redefine
try {
  Object.defineProperty(obj, "readonly", { value: 0 });
} catch (e: any) {
  console.log("caught: " + e.constructor.name);
}

// getOwnPropertyDescriptor
const desc = Object.getOwnPropertyDescriptor(obj, "readonly")!;
console.log(desc.value);
console.log(desc.writable);
console.log(desc.enumerable);
console.log(desc.configurable);

// Object.getOwnPropertyDescriptors
const all = Object.getOwnPropertyDescriptors({ x: 1, y: 2 });
console.log(all.x.value);
console.log(all.x.writable);
