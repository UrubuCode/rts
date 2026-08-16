// Cross-runtime: object spread copies enumerable values rather than descriptors.
let reads = 0;
const source = Object.defineProperty({}, "x", {
  enumerable: true,
  get() { reads++; return 7; },
});
const copy: any = { ...source };
const d = Object.getOwnPropertyDescriptor(copy, "x")!;
console.log(copy.x, reads);
console.log(d.value, d.writable, d.enumerable, d.configurable, typeof d.get);

