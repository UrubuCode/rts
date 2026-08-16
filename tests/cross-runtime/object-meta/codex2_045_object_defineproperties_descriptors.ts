// Cross-runtime: defineProperties installs data and accessor descriptors together.
const o: any = {};
let cell = 3;
Object.defineProperties(o, {
  fixed: { value: 7, enumerable: true },
  live: { get() { return cell; }, set(v) { cell = v * 2; }, enumerable: false },
});
o.live = 5;
console.log(o.fixed, o.live, Object.keys(o).join(","));
console.log(JSON.stringify(Object.getOwnPropertyDescriptors(o), (k, v) => typeof v === "function" ? "fn" : v));

