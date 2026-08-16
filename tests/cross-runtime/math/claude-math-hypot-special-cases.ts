// ONE thing: Math.hypot's special-case order. An Infinity anywhere in the list
// wins over a NaN anywhere else — the spec checks for infinities FIRST — and
// every argument is still coerced before either check runs.

function hyp(label: string, args: number[]): void {
  const r = Math.hypot.apply(null, args as any);
  console.log(
    label + " hypot(" + args.map(String).join(",") + ")" +
      " = " + String(r) +
      " neg0:" + Object.is(r, -0)
  );
}

// --- the identity element and the single-argument case ---
hyp("empty", []);
hyp("one_pos", [3]);
hyp("one_neg", [-3]);
hyp("one_zero", [0]);
hyp("one_negzero", [-0]);
hyp("one_nan", [NaN]);
hyp("one_inf", [Infinity]);
hyp("one_neginf", [-Infinity]);

// --- Infinity beats NaN, in either position ---
hyp("inf_then_nan", [Infinity, NaN]);
hyp("nan_then_inf", [NaN, Infinity]);
hyp("neginf_then_nan", [-Infinity, NaN]);
hyp("nan_nan_inf", [NaN, NaN, -Infinity]);
hyp("nan_only", [NaN, 1]);
hyp("nan_nan", [NaN, NaN]);
hyp("inf_inf", [Infinity, -Infinity]);
hyp("inf_finite", [Infinity, 3]);

// --- every zero collapses to +0, never -0 ---
hyp("zeros", [0, 0]);
hyp("negzeros", [-0, -0]);
hyp("mixed_zeros", [0, -0]);
hyp("three_negzeros", [-0, -0, -0]);

// --- exact Pythagorean results ---
hyp("3_4", [3, 4]);
hyp("5_12", [5, 12]);
hyp("8_15", [8, 15]);
hyp("7_24", [7, 24]);
hyp("20_21", [20, 21]);
hyp("3_4_12", [3, 4, 12]);
hyp("1_2_2", [1, 2, 2]);
hyp("2_3_6", [2, 3, 6]);
hyp("neg3_neg4", [-3, -4]);
hyp("0_5", [0, 5]);
hyp("neg0_neg5", [-0, -5]);

// --- the point of hypot: no intermediate overflow or underflow ---
console.log("--- scaling ---");
console.log("big_finite=" + isFinite(Math.hypot(1e200, 1e200)));
console.log("big_naive_overflows=" + !isFinite(Math.sqrt(1e200 * 1e200 + 1e200 * 1e200)));
console.log("small_nonzero=" + (Math.hypot(1e-200, 1e-200) > 0));
console.log("small_naive_underflows=" + (Math.sqrt(1e-200 * 1e-200 + 1e-200 * 1e-200) === 0));
console.log("big_scaled_exact=" + (Math.hypot(3e300, 4e300) === 5e300));
// NOTE: the small-end counterpart, Math.hypot(3e-300, 4e-300) === 5e-300, is
// NOT asserted: JavaScriptCore answers 5.0000000000000006e-300 where V8 answers
// 5e-300. The scaled sum is implementation-approximated, so only "it did not
// underflow to zero" is checked above.
console.log("small_scaled_close=" + (Math.abs(Math.hypot(3e-300, 4e-300) / 5e-300 - 1) < 1e-12));
console.log("maxvalue_finite=" + isFinite(Math.hypot(Number.MAX_VALUE, 0)));
console.log("maxvalue_pair_overflows=" + !isFinite(Math.hypot(Number.MAX_VALUE, Number.MAX_VALUE)));

// --- coercion happens before the Infinity check, so a throw wins ---
console.log("--- coercion ---");
console.log("strings=" + String(Math.hypot("3" as any, "4" as any)));
console.log("null=" + String(Math.hypot(null as any, 5)));
console.log("undefined=" + String(Math.hypot(undefined as any, 5)));
console.log("bool=" + String(Math.hypot(true as any, 0)));
console.log("arr=" + String(Math.hypot([3] as any, [4] as any)));

const log: string[] = [];
const boom: any = { valueOf: function () { log.push("boom"); throw new TypeError("no"); } };
try {
  console.log("with_inf_and_throw=" + String(Math.hypot(Infinity, boom, { valueOf: function () { log.push("c"); return 1; } } as any)));
} catch (e) {
  console.log("threw=" + (e as any).constructor.name);
}
console.log("log=" + log.join(","));

try {
  console.log("symbol=" + String(Math.hypot(1, Symbol("s") as any)));
} catch (e) {
  console.log("symbol_threw=" + (e as any).constructor.name);
}

console.log("length=" + Math.hypot.length);
console.log("name=" + Math.hypot.name);
