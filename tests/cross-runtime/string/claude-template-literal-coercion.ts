const obj = {
  toString() { return "T"; },
  valueOf() { return 42; },
};
console.log(`val=${obj}`);
const arr = [1, 2, 3];
console.log(`arr=${arr}`);
const nested = [[1, 2], [3, 4]];
console.log(`nested=${nested}`);
const o2 = { toString() { return "only-str"; } };
console.log(`o2=${o2}`);
console.log(`${obj} and ${obj + 1}`);
console.log(`null=${null} undef=${undefined}`);
const sym = { [Symbol.toPrimitive](h: string) { return h === "string" ? "S" : 99; } };
console.log(`sym=${sym}`);