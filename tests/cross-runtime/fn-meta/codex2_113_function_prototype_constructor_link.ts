// Cross-runtime: ordinary functions have a prototype linked back by constructor.
function Thing(this: any) {}
const d = Object.getOwnPropertyDescriptor(Thing, "prototype")!;
console.log(typeof Thing.prototype, Thing.prototype.constructor === Thing);
console.log(d.writable, d.enumerable, d.configurable);
console.log(Object.hasOwn(() => 1, "prototype"));

