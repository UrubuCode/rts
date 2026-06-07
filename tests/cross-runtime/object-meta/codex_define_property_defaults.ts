// Cross-runtime: descriptor defaults for defineProperty.
const obj: any = {};
Object.defineProperty(obj, "x", { value: 1 });
const d = Object.getOwnPropertyDescriptor(obj, "x")!;
console.log([d.value, d.writable, d.enumerable, d.configurable].join(","));
console.log(Object.keys(obj).join(","));

try { obj.x = 2; } catch (_) {}
console.log(obj.x);

try {
  Object.defineProperty(obj, "x", { configurable: true });
} catch (e: any) {
  console.log(e.constructor.name);
}
