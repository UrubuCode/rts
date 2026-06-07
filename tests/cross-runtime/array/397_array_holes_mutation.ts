// Cross-runtime: sparse array holes through mutation and callbacks.
const a = [0, , 2, , 4] as any[];
const seen: string[] = [];

a.forEach((v, i) => {
  seen.push(i + "=" + v);
  if (i === 0) a[1] = 11;
  if (i === 2) delete a[4];
});

console.log("seen=" + seen.join(","));
console.log("keys=" + Object.keys(a).join(","));
console.log("map=" + a.map((v, i) => i + ":" + v).join("|"));
console.log("join=" + a.join("-"));
console.log("has1=" + (1 in a) + ",has3=" + (3 in a) + ",has4=" + (4 in a));
