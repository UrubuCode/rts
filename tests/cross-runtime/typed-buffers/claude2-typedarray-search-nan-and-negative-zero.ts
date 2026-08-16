// Cross-runtime: the searching and element-writing methods of a typed array over
// the two values that break equality — NaN and -0. indexOf uses strict equality
// (NaN never matches), includes uses SameValueZero (it does), and -0 matches 0.

const f = new Float64Array([NaN, -0, 0, 1, NaN]);

console.log("indexOf_nan=" + f.indexOf(NaN) + " lastIndexOf_nan=" + f.lastIndexOf(NaN) + " includes_nan=" + f.includes(NaN));
console.log("indexOf_zero=" + f.indexOf(0) + " indexOf_negzero=" + f.indexOf(-0) + " lastIndexOf_negzero=" + f.lastIndexOf(-0));
console.log("includes_negzero=" + f.includes(-0) + " includes_zero=" + f.includes(0));
console.log("negzero_stored=" + Object.is(f[1], -0) + " positive=" + Object.is(f[2], 0));
console.log("find_nan=" + String(f.find(function (v) { return Number.isNaN(v); })) + " findIndex=" + f.findIndex(function (v) { return Number.isNaN(v); }));
console.log("findLast=" + f.findLastIndex(function (v) { return Number.isNaN(v); }));
console.log("indexOf_fromIndex=" + f.indexOf(1, 4) + "," + f.indexOf(1, -2) + "," + f.indexOf(1, -99) + "," + f.indexOf(1, 99));
console.log("includes_fromIndex=" + f.includes(1, 4) + "," + f.includes(1, -2));
console.log("lastIndexOf_fromIndex=" + f.lastIndexOf(NaN, 0) + "," + f.lastIndexOf(0, 1));

// An integer kind cannot hold either value, so the same searches change answer.
const i = new Int32Array([NaN as any, -0, 5]);
console.log("int_stored=" + i.join(",") + " negzero_is_zero=" + Object.is(i[1], -0));
console.log("int_search=" + i.indexOf(NaN) + "," + i.includes(NaN) + "," + i.indexOf(-0) + "," + i.indexOf(0));
console.log("int_search_string=" + i.indexOf("5" as any) + " includes_string=" + i.includes("5" as any));
console.log("float_search_string=" + f.indexOf("1" as any));

const t = function (fn: () => any): string {
  try {
    return String(fn());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// at() takes a relative index and never wraps twice.
console.log("at=" + String(f.at(0)) + "," + String(f.at(-1)) + "," + String(f.at(-9)) + "," + String(f.at(9)) + "," + String(f.at(1.9)) + "," + String(f.at(NaN as any)));

// fill() coerces once, clamps its range, and answers the same object.
console.log("fill_all=" + new Uint8Array(4).fill(300).join(","));
console.log("fill_range=" + new Uint8Array([1, 2, 3, 4]).fill(9, 1, 3).join(","));
console.log("fill_negative=" + new Uint8Array([1, 2, 3, 4]).fill(9, -2).join(","));
console.log("fill_inverted=" + new Uint8Array([1, 2, 3, 4]).fill(9, 3, 1).join(","));
console.log("fill_past_end=" + new Uint8Array([1, 2]).fill(9, 0, 99).join(","));
console.log("fill_identity=" + t(function () { const a = new Uint8Array(2); return String(a.fill(1) === a); }));
console.log("fill_nan_into_int=" + new Int8Array(2).fill(NaN as any).join(","));
console.log("fill_valueof=" + t(function () {
  let calls = 0;
  const a = new Uint8Array(3);
  a.fill({ valueOf: function () { calls++; return 4; } } as any);
  return a.join(",") + "/calls:" + calls;
}));

// copyWithin moves bytes inside the same buffer, overlap included.
console.log("copyWithin=" + new Uint8Array([1, 2, 3, 4, 5]).copyWithin(0, 3).join(","));
console.log("copyWithin_overlap=" + new Uint8Array([1, 2, 3, 4, 5]).copyWithin(1, 0, 4).join(","));
console.log("copyWithin_negative=" + new Uint8Array([1, 2, 3, 4, 5]).copyWithin(-2, 0, 2).join(","));
console.log("copyWithin_noop=" + new Uint8Array([1, 2, 3]).copyWithin(0, 0).join(","));

// with() answers a copy and refuses an index outside the range.
console.log("with=" + t(function () { return f.with(0, 5).join(","); }));
console.log("with_negative=" + t(function () { return new Uint8Array([1, 2, 3]).with(-1, 9).join(","); }));
console.log("with_oob=" + t(function () { return (new Uint8Array([1, 2, 3]) as any).with(3, 9); }));
console.log("with_oob_negative=" + t(function () { return (new Uint8Array([1, 2, 3]) as any).with(-4, 9); }));
console.log("with_coerces=" + t(function () { return new Uint8Array([1, 2]).with(0, 300 as any).join(","); }));
console.log("with_keeps_source=" + t(function () {
  const a = new Uint8Array([1, 2]);
  const b = a.with(0, 9);
  return a.join(",") + "/" + b.join(",") + "/" + (a.buffer === b.buffer) + "/" + b.constructor.name;
}));
console.log("with_nan_into_int=" + t(function () { return new Int8Array([1]).with(0, NaN as any).join(","); }));
