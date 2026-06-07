const a = [10, 20, 30];
console.log(a.reduce((acc, x) => acc + x));
console.log(a.reduceRight((acc, x) => acc + "|" + x, ""));
const single = [42];
console.log(single.reduce((acc, x) => acc + x));
const sparse = [1, , 3];
console.log(sparse.reduce((acc, x) => acc + x));
try {
  ([] as number[]).reduce((acc, x) => acc + x);
} catch (e) {
  console.log(e instanceof TypeError);
}
console.log([] .reduce((acc, x) => acc + x, 99));
