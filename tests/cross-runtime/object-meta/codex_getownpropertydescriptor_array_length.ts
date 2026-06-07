// Cross-runtime: array length descriptor has special attributes.
const a = [1, 2, 3];
const d = Object.getOwnPropertyDescriptor(a, "length")!;
console.log(d.value + ":" + d.writable + ":" + d.enumerable + ":" + d.configurable);
Object.defineProperty(a, "length", { writable: false });
console.log(Object.getOwnPropertyDescriptor(a, "length")!.writable);
try {
  a.push(4);
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log(a.join(","));
