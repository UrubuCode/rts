// Cross-runtime: sort comparator coercion and stability with odd returns.
const items = [
  { k: 2, id: "a" },
  { k: 1, id: "b" },
  { k: 2, id: "c" },
  { k: 1, id: "d" }
];

items.sort((x, y) => (x.k === y.k ? NaN : x.k < y.k ? "-1" as any : true as any));
console.log(items.map(x => x.k + x.id).join(","));

const nums = [3, 2, 10, 1];
nums.sort();
console.log(nums.join(","));
