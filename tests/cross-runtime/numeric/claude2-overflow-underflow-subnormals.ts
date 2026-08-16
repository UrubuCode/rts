// ONE thing: the two ends of the double's exponent range. Above MAX_VALUE the
// result saturates to Infinity with the sign kept; below MIN_VALUE it degrades
// through the subnormals and then underflows to a SIGNED zero — and the
// rounding at both edges is ties-to-even, which is why MIN_VALUE/2 is exactly 0.

// --- the top: where a finite result stops being finite ---
console.log("max_times_2=" + String(Number.MAX_VALUE * 2));
console.log("max_plus_max=" + String(Number.MAX_VALUE + Number.MAX_VALUE));
console.log("neg_max_times_2=" + String(-Number.MAX_VALUE * 2));
console.log("max_squared=" + String(Number.MAX_VALUE * Number.MAX_VALUE));
console.log("max_times_1p1=" + String(Number.MAX_VALUE * 1.1));
console.log("max_plus_one=" + (Number.MAX_VALUE + 1 === Number.MAX_VALUE));
console.log("max_plus_1e291=" + (Number.MAX_VALUE + 1e291 === Number.MAX_VALUE));
console.log("max_plus_1e292=" + String(Number.MAX_VALUE + 1e292));
console.log("max_next_is_infinity=" + (Number.MAX_VALUE * (1 + Number.EPSILON) === Infinity));
console.log("half_max_doubled=" + (Number.MAX_VALUE / 2 * 2 === Number.MAX_VALUE));
console.log("1e308_times_10=" + String(1e308 * 10));
console.log("2p1023_times_2=" + String(2 ** 1023 * 2));
console.log("max_is_finite=" + Number.isFinite(Number.MAX_VALUE) + " next_is=" + Number.isFinite(Number.MAX_VALUE * 2));

// --- infinity arithmetic once you are there ---
console.log("inf_minus_inf=" + String(Infinity - Infinity));
console.log("inf_times_zero=" + String(Infinity * 0));
console.log("inf_over_inf=" + String(Infinity / Infinity));
console.log("inf_over_finite=" + String(Infinity / 1e308));
console.log("finite_over_inf=" + String(1e308 / Infinity) + " neg0:" + Object.is(1e308 / Infinity, -0));
console.log("neg_finite_over_inf=" + String(-1e308 / Infinity) + " neg0:" + Object.is(-1e308 / Infinity, -0));
console.log("inf_plus_finite=" + String(Infinity + 1e308));
console.log("inf_mod=" + String(Infinity % 2) + " mod_inf=" + String(2 % Infinity));

// --- the bottom: MIN_VALUE is the smallest SUBNORMAL, not the smallest normal ---
console.log("min_value=" + String(Number.MIN_VALUE));
console.log("min_value_is_2p_neg1074=" + (Number.MIN_VALUE === 2 ** -1074));
console.log("smallest_normal=" + String(2 ** -1022));
console.log("largest_subnormal=" + String(2 ** -1022 - 2 ** -1074));
console.log("subnormal_is_smaller=" + (2 ** -1022 - 2 ** -1074 < 2 ** -1022));
console.log("min_normal_over_2_is_subnormal=" + String(2 ** -1022 / 2));

// --- underflow: half of the smallest subnormal is a tie, and ties go to even ---
console.log("min_over_2=" + String(Number.MIN_VALUE / 2) + " is_pos_zero:" + Object.is(Number.MIN_VALUE / 2, 0));
console.log("neg_min_over_2=" + String(-Number.MIN_VALUE / 2) + " is_neg_zero:" + Object.is(-Number.MIN_VALUE / 2, -0));
console.log("min_times_0p6=" + String(Number.MIN_VALUE * 0.6));
console.log("min_times_0p4=" + String(Number.MIN_VALUE * 0.4));
console.log("min_over_3=" + String(Number.MIN_VALUE / 3));
console.log("min_over_2_times_2=" + String(Number.MIN_VALUE / 2 * 2));
console.log("three_halves_of_min=" + String(Number.MIN_VALUE * 1.5));
console.log("tiny_product_signs=" + String(1e-320 * 1e-10) + "," + String(-1e-320 * 1e-10) + "," + Object.is(-1e-320 * 1e-10, -0));

// --- inside the subnormal range, precision is already gone ---
const sub = Number.MIN_VALUE * 3;
console.log("sub_times3=" + String(sub));
console.log("sub_div3_roundtrip=" + (sub / 3 === Number.MIN_VALUE));
console.log("subnormal_gap_is_constant=" + (2 ** -1074 * 2 - 2 ** -1074 === 2 ** -1074));
console.log("subnormals_are_finite=" + Number.isFinite(Number.MIN_VALUE) + "," + Number.isFinite(Number.MIN_VALUE * 7));
console.log("subnormal_is_not_zero=" + (Number.MIN_VALUE !== 0) + " truthy=" + (Number.MIN_VALUE ? "yes" : "no"));
console.log("subnormal_toFixed=" + Number.MIN_VALUE.toFixed(3));
console.log("subnormal_exponential=" + Number.MIN_VALUE.toExponential(3));

// --- adding a tiny value to 1 is absorbed long before underflow ---
console.log("one_plus_min=" + (1 + Number.MIN_VALUE === 1));
console.log("one_plus_eps=" + (1 + Number.EPSILON === 1));
console.log("one_plus_half_eps=" + (1 + Number.EPSILON / 2 === 1));
console.log("one_plus_half_eps_plus_min=" + (1 + (Number.EPSILON / 2 + Number.MIN_VALUE) === 1));

// --- the sign survives both edges ---
const edges: [string, number][] = [
  ["+overflow", 1e308 * 10],
  ["-overflow", -1e308 * 10],
  ["+underflow", 1e-320 * 1e-10],
  ["-underflow", -1e-320 * 1e-10],
];
for (const e of edges) {
  console.log(e[0] + "=" + String(e[1]) + " sign=" + String(Math.sign(e[1])) + " neg0=" + Object.is(e[1], -0));
}

// --- and the parser saturates the same way as the arithmetic ---
console.log("parse_overflow=" + String(Number("1e400")) + "," + String(Number("-1e400")));
console.log("parse_underflow=" + String(Number("1e-400")) + " neg0:" + Object.is(Number("-1e-400"), -0));
console.log("parse_max_boundary=" + String(Number("1.7976931348623157e308")) + "," + String(Number("1.7976931348623159e308")));
console.log("literal_overflow=" + String(1e400) + "," + String(-1e400));
console.log("literal_underflow=" + String(1e-400) + " neg0:" + Object.is(-1e-400, -0));
