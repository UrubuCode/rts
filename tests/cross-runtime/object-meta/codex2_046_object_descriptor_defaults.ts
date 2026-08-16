// Cross-runtime: omitted descriptor flags default to false.
const o: any = {};
Object.defineProperty(o, "x", { value: 1 });
const d = Object.getOwnPropertyDescriptor(o, "x")!;
console.log(d.value, d.writable, d.enumerable, d.configurable);
console.log(Object.keys(o).length, Object.hasOwn(o, "x"));

