// Cross-runtime: Object.getOwnPropertyDescriptors
const obj = {
  a: 1,
  get b() { return 2; },
};

Object.defineProperty(obj, "c", {
  value: 3,
  writable: false,
  enumerable: false,
});

const descs = Object.getOwnPropertyDescriptors(obj);
console.log("keys=" + Object.keys(descs).sort().join(","));
console.log("a.value=" + descs.a.value);
console.log("a.writable=" + descs.a.writable);
console.log("a.enumerable=" + descs.a.enumerable);
console.log("b.get=" + (typeof descs.b.get));
console.log("c.value=" + descs.c.value);
console.log("c.writable=" + descs.c.writable);
console.log("c.enumerable=" + descs.c.enumerable);
