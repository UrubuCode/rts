// ONE thing: Math.max/min coerce EVERY argument before comparing any of them.
// So a throwing valueOf in position 2 stops position 3 from being coerced and
// no result is produced at all — even when position 1 is already NaN.

function fmt(v: number): string {
  return String(v) + (Object.is(v, -0) ? "(neg0)" : "");
}

// --- comparison is numeric, never lexicographic ---
console.log("str_max=" + fmt(Math.max("3" as any, "10" as any)));
console.log("str_min=" + fmt(Math.min("3" as any, "10" as any)));
console.log("mixed_max=" + fmt(Math.max(true as any, null as any, "2" as any)));
console.log("arr_max=" + fmt(Math.max([] as any, [5] as any)));
console.log("obj_max=" + fmt(Math.max({} as any)));
console.log("undef_max=" + fmt(Math.max(undefined as any, 1)));
console.log("emptystr_min=" + fmt(Math.min("" as any, 1)));
console.log("hex_max=" + fmt(Math.max("0x10" as any, 5)));
console.log("wrapper_max=" + fmt(Math.max(new Number(7) as any, 3)));

// --- every argument is coerced, in order, even after NaN appears ---
const log1: string[] = [];
function spy(name: string, value: number): any {
  return { valueOf: function () { log1.push(name); return value; } };
}
console.log("with_nan=" + fmt(Math.max(NaN, spy("a", 1), spy("b", 2))));
console.log("coerced_after_nan=" + log1.join(","));

const log2: string[] = [];
function spy2(name: string, value: number): any {
  return { valueOf: function () { log2.push(name); return value; } };
}
console.log("plain=" + fmt(Math.max(spy2("a", 5), spy2("b", 9), spy2("c", 1))));
console.log("order=" + log2.join(","));

// --- a throwing valueOf in the middle prevents the rest AND the result ---
const log3: string[] = [];
const boom: any = {
  valueOf: function () {
    log3.push("boom");
    throw new TypeError("no");
  },
};
function spy3(name: string, value: number): any {
  return { valueOf: function () { log3.push(name); return value; } };
}
try {
  console.log("never=" + fmt(Math.max(spy3("a", 1), boom, spy3("c", 3))));
} catch (e) {
  console.log("threw=" + (e as any).constructor.name);
}
console.log("log_after_throw=" + log3.join(","));

// --- the same rule for Math.min ---
const log4: string[] = [];
const boom4: any = {
  valueOf: function () {
    log4.push("boom");
    throw new RangeError("no");
  },
};
try {
  Math.min({ valueOf: function () { log4.push("a"); return 1; } } as any, boom4, { valueOf: function () { log4.push("c"); return 3; } } as any);
  console.log("min_no_throw");
} catch (e) {
  console.log("min_threw=" + (e as any).constructor.name);
}
console.log("min_log=" + log4.join(","));

// --- a Symbol argument throws at ToNumber, from the same loop ---
try {
  console.log("symbol=" + fmt(Math.max(1, Symbol("s") as any)));
} catch (e) {
  console.log("symbol_threw=" + (e as any).constructor.name);
}
try {
  console.log("bigint=" + fmt(Math.max(1, 2n as any)));
} catch (e) {
  console.log("bigint_threw=" + (e as any).constructor.name);
}

// --- Symbol.toPrimitive is asked with hint "number" ---
const hints: string[] = [];
const hinted: any = {
  [Symbol.toPrimitive]: function (h: string) { hints.push(h); return 42; },
};
console.log("hinted=" + fmt(Math.max(1, hinted)));
console.log("hint=" + hints.join(","));

// --- spread and array-like argument lists ---
console.log("spread=" + fmt(Math.max(...[3, 1, 4, 1, 5])));
console.log("apply=" + fmt(Math.max.apply(null, [3, 1, 4, 1, 5])));
console.log("apply_empty=" + fmt(Math.max.apply(null, [])));
console.log("min_spread=" + fmt(Math.min(...[3, 1, 4, 1, 5])));
console.log("max_length=" + Math.max.length);
console.log("min_length=" + Math.min.length);
console.log("max_name=" + Math.max.name);

// --- the identity elements, and one NaN is enough to win ---
console.log("max_empty=" + fmt(Math.max()));
console.log("min_empty=" + fmt(Math.min()));
console.log("max_nan_last=" + fmt(Math.max(1, 2, NaN)));
console.log("min_nan_first=" + fmt(Math.min(NaN, 1, 2)));
console.log("max_inf=" + fmt(Math.max(Infinity, NaN)));
console.log("min_neginf=" + fmt(Math.min(-Infinity, NaN)));
