// Cross-runtime: reduce finds the first present sparse element as its initial accumulator.
const a: any[] = [];
a.length = 7;
a[3] = 4;
a[6] = 7;
const seen: string[] = [];
const result = a.reduce((acc, value, index) => {
  seen.push(index + ":" + acc + ":" + value);
  return acc + value;
});
console.log(result);
console.log(seen.join("|"));

