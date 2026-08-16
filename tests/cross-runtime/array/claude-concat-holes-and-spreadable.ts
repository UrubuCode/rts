// ONE thing: concat SPREADS arrays one level and PRESERVES their holes, while
// anything non-spreadable is appended whole.
const h: any[] = [1, , 3];
const r = h.concat([4, , 6]);
console.log("len=" + r.length);
console.log("in=" + [0, 1, 2, 3, 4, 5].map((i) => (i in r ? "y" : "n")).join(""));
console.log("v=" + r.map(String).join(","));

console.log("oneLevel=" + JSON.stringify([1].concat([[2, 3]] as any)));
console.log("mixed=" + JSON.stringify(([1] as any).concat(2, [3, 4], [[5]])));
console.log("arrayLike=" + JSON.stringify(([0] as any).concat({ length: 2, 0: "a" })));
console.log("string=" + JSON.stringify(([0] as any).concat("ab")));

const spreadable: any = { length: 2, 0: "x", 1: "y" };
spreadable[Symbol.isConcatSpreadable] = true;
console.log("forced=" + JSON.stringify(([0] as any).concat(spreadable)));

const noSpread: any = [1, 2];
noSpread[Symbol.isConcatSpreadable] = false;
const ns = ([0] as any).concat(noSpread);
console.log("blockedLen=" + ns.length + " isArr=" + Array.isArray(ns[1]));

const zeroFlag: any = [7];
zeroFlag[Symbol.isConcatSpreadable] = 0;
console.log("zeroFlag=" + ([0] as any).concat(zeroFlag).length);

const copy = h.concat();
console.log("copyIn=" + [0, 1, 2].map((i) => (i in copy ? "y" : "n")).join("") + " same=" + (copy === h));

const short: any = { length: 1, 0: "s", [Symbol.isConcatSpreadable]: true };
console.log("short=" + JSON.stringify(([] as any).concat(short)));
