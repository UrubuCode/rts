const obj = { b: 1, a: 2, "2": 3, "1": 4 };
console.log(JSON.stringify(obj, ["a", "1", "missing"]));
console.log(JSON.stringify(obj, null, 2));
const data = { x: 1, secret: "hide", y: 2 };
const out = JSON.stringify(data, (k, v) =>
  k === "secret" ? undefined : v
);
console.log(out);
const nums = JSON.stringify({ a: 1, b: 2, c: 3 }, (k, v) =>
  typeof v === "number" ? v * 10 : v
);
console.log(nums);
console.log(JSON.stringify({ a: [1, 2] }, null, "\t"));
