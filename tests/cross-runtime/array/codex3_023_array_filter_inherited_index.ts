// Cross-runtime: array iteration methods observe inherited numeric properties in holes.
const proto: any = { 1: "inherited" };
const a: any = Object.create(proto);
a[0] = "own";
a[2] = "last";
a.length = 3;
const seen: string[] = [];
const out = Array.prototype.filter.call(a, (v: any, i: number) => {
  seen.push(i + ":" + v);
  return true;
});
console.log(seen.join("|"));
console.log(JSON.stringify(out));

