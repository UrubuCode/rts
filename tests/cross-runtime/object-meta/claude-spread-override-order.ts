const a = { x: 1, y: 2 };
const b = { y: 3, z: 4 };
const merged = { ...a, y: 99, ...b };
console.log(JSON.stringify(merged));
console.log(Object.keys(merged).join(","));
let log = [];
const src = {
  get p() { log.push("p"); return 1; },
  get q() { log.push("q"); return 2; },
};
const out = { first: 0, ...src, last: 3 };
console.log(Object.keys(out).join(","));
console.log(log.join(","));
const withNull = { ...null, ...undefined, ...{ a: 1 }, ...{} };
console.log(JSON.stringify(withNull));
const strSpread = { ..."ab" };
console.log(JSON.stringify(strSpread));
