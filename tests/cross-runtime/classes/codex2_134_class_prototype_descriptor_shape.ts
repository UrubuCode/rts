// Cross-runtime: class prototype methods are writable, non-enumerable, configurable.
class Shape { area() { return 12; } }
const d = Object.getOwnPropertyDescriptor(Shape.prototype, "area")!;
console.log(typeof d.value, d.writable, d.enumerable, d.configurable);
console.log(Object.keys(Shape.prototype).length);

