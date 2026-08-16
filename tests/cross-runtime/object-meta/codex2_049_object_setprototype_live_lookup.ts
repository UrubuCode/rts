// Cross-runtime: setPrototypeOf changes subsequent inherited lookup.
const a = { value: "a" };
const b = { value: "b", extra: 2 };
const o: any = Object.create(a);
console.log(o.value, o.extra);
Object.setPrototypeOf(o, b);
console.log(o.value, o.extra, Object.getPrototypeOf(o) === b);

