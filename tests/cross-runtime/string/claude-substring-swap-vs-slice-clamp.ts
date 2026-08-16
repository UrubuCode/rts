// Cross-runtime: substring SWAPS its arguments when start > end and clamps a
// negative index to 0; slice does neither — it treats a negative index as an
// offset from the end and returns "" when start >= end. Same two numbers, two
// different answers. 168/215 test each method alone, never the pair side by side.

const s = "abcdef"; // indices 0..5

function pair(a: number | undefined, b: number | undefined): string {
  return "[" + s.substring(a as any, b as any) + "][" + s.slice(a as any, b as any) + "]";
}

// --- ordered range: identical ---
console.log("1-4=" + pair(1, 4));
console.log("0-6=" + pair(0, 6));
console.log("2-2=" + pair(2, 2));

// --- reversed range: substring swaps, slice yields empty ---
console.log("4-1=" + pair(4, 1));
console.log("6-0=" + pair(6, 0));
console.log("3-2=" + pair(3, 2));

// --- negative start: substring clamps to 0, slice counts from the end ---
console.log("neg3-2=" + pair(-3, 2));
console.log("neg3-und=" + pair(-3, undefined));
console.log("neg1-und=" + pair(-1, undefined));
console.log("neg99-und=" + pair(-99, undefined));

// --- negative end ---
console.log("1-neg1=" + pair(1, -1));
console.log("0-neg99=" + pair(0, -99));
console.log("neg4-neg2=" + pair(-4, -2));

// --- out of range beyond the end ---
console.log("2-99=" + pair(2, 99));
console.log("99-2=" + pair(99, 2));
console.log("99-und=" + "[" + s.substring(99) + "][" + s.slice(99) + "]");

// --- NaN becomes 0 for both ---
console.log("nan-3=" + pair(NaN, 3));
console.log("3-nan=" + pair(3, NaN));

// --- undefined end means "to the end"; undefined start means 0 ---
console.log("und-3=" + pair(undefined, 3));
console.log("2-und=" + pair(2, undefined));
console.log("noargs=" + "[" + s.substring() + "][" + s.slice() + "]");

// --- infinities ---
console.log("0-inf=" + pair(0, Infinity));
console.log("inf-2=" + pair(Infinity, 2));
console.log("neginf-2=" + pair(-Infinity, 2));
console.log("0-neginf=" + pair(0, -Infinity));

// --- fractions truncate toward zero ---
console.log("frac=" + pair(1.9, 4.9));
console.log("frac-neg=" + pair(-1.9, undefined));

// --- -0 behaves as 0 for both ---
console.log("negzero=" + "[" + s.substring(-0) + "][" + s.slice(-0) + "]");
console.log("negzero-end=" + pair(0, -0));

// --- arguments go through ToInteger, so strings and objects are accepted ---
console.log("str-args=" + "[" + s.substring("2" as any, "4" as any) + "][" + s.slice("2" as any, "4" as any) + "]");
console.log("bool-args=" + "[" + s.substring(true as any, 3 as any) + "][" + s.slice(true as any, 3 as any) + "]");
const boxed: any = { valueOf() { return 1; } };
console.log("obj-args=" + "[" + s.substring(boxed, 3) + "][" + s.slice(boxed, 3) + "]");
console.log("null-args=" + "[" + s.substring(null as any, 3) + "][" + s.slice(null as any, 3) + "]");

// --- the empty string never produces anything ---
console.log("empty=" + "[" + "".substring(-5, 5) + "][" + "".slice(-5, 5) + "]");

// --- substring is not slice: the swap is observable through length ---
console.log("len-swap=" + s.substring(5, 1).length + ":" + s.slice(5, 1).length);
