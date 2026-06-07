const obj = { factor: 3 };
const r = Array.from([1, 2, 3], function (this: any, x) { return x * this.factor; }, obj);
console.log(r.join(","));
console.log(Array.from({ length: 3 }, (_, i) => i * i).join(","));
console.log(Array.from("abc").join("-"));
console.log(Array.from({ length: 4 }).length);
console.log(Array.from([5, 6, 7], (x, i) => x + i).join(","));
console.log(Array.from(new Set([1, 1, 2, 3, 3])).join(","));
