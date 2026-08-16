// Cross-runtime: boxed strings expose immutable indexed own properties.
const boxed = new String("abc");
console.log(boxed.length, boxed[1], Object.keys(boxed).join(","));
const d = Object.getOwnPropertyDescriptor(boxed, "1")!;
console.log(d.value, d.writable, d.enumerable, d.configurable);
console.log(boxed.valueOf() === "abc");

