// Cross-runtime compatibility: nested destructuring and defaults.
const arr = [1, , 3, [4, 5]];
const [a, b = 20, c, [d, e = 50]] = arr as any;
console.log("arr=" + [a, b, c, d, e].join(","));

const obj = { x: 1, inner: { y: 2 }, extra: 9 };
const {
  x,
  inner: { y, z = 7 },
  miss = 8,
  ...rest
} = obj as any;
console.log("obj=" + [x, y, z, miss].join(","));
console.log("rest=" + JSON.stringify(rest));
