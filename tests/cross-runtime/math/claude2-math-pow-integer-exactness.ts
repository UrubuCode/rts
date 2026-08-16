// ONE thing: where exponentiation is EXACT. A power of two is exact across the
// whole double range, a power of ten only up to 10^22, and past those the
// result is the nearest double — which is why 10^23 is not the integer it
// prints as. The overflow and underflow edges are pinned too.

// --- every power of two agrees with repeated doubling, over the full range ---
const badPow2: string[] = [];
let acc = 1;
for (let k = 0; k <= 1023; k++) {
  if (Math.pow(2, k) !== acc || 2 ** k !== acc) badPow2.push(String(k));
  acc = acc * 2;
}
console.log("pow2_failures=[" + badPow2.join(",") + "]");
console.log("pow2_1023=" + String(Math.pow(2, 1023)));
console.log("pow2_1024=" + String(Math.pow(2, 1024)));
console.log("pow2_53=" + String(2 ** 53));
console.log("pow2_53_plus_one_lost=" + (2 ** 53 === 2 ** 53 + 1));

// --- negative powers of two are exact down into the subnormals ---
const badNeg: string[] = [];
for (let k = 0; k <= 1023; k++) {
  if (Math.pow(2, -k) !== 1 / Math.pow(2, k)) badNeg.push(String(k));
}
console.log("neg_pow2_failures=[" + badNeg.join(",") + "]");
console.log("pow2_neg1074=" + String(2 ** -1074));
console.log("pow2_neg1075=" + String(2 ** -1075));
console.log("pow2_neg1075_is_pos_zero=" + Object.is(2 ** -1075, 0));

// --- powers of ten: exact integers only through 10^22 ---
const ten: string[] = [];
for (let k = 0; k <= 25; k++) {
  ten.push(String(k) + ":" + String(10 ** k));
}
console.log("pow10=" + ten.join(" "));
console.log("10e22_is_integer_exact=" + (10 ** 22 === 10000000000000000000000));
console.log("10e23_digits=" + (10 ** 23).toFixed(0));
console.log("10e22_digits=" + (10 ** 22).toFixed(0));
console.log("pow10_matches_literal=" + (10 ** 23 === 1e23) + "," + (10 ** 22 === 1e22));
// (10 ** 308 is NOT asserted: it is genuinely implementation-defined at that
// magnitude — JavaScriptCore answers 1.0000000000000006e+308 and V8 answers
// 1e+308, measured. Only the overflow to Infinity above it is pinned.)
console.log("pow10_309=" + String(10 ** 309));
console.log("pow10_308_is_finite=" + Number.isFinite(10 ** 308));

// --- Math.pow and ** are the same operation ---
const disagree: string[] = [];
const bases: number[] = [2, 3, 10, -2, 0.5, 1.5];
for (const b of bases) {
  for (let k = -5; k <= 20; k++) {
    if (!Object.is(Math.pow(b, k), b ** k)) disagree.push(b + "^" + k);
  }
}
console.log("method_vs_operator_disagree=[" + disagree.join(",") + "]");

// --- a negative base: the parity of an integer exponent decides the sign ---
const signs: string[] = [];
for (let k = 0; k <= 8; k++) {
  signs.push(k + ":" + String((-2) ** k));
}
console.log("neg_base=" + signs.join(" "));
console.log("neg_base_frac_exponent=" + String((-2) ** 0.5));
console.log("neg_base_neg_exponent_odd=" + String((-2) ** -3));
console.log("neg_zero_odd=" + String((-0) ** 3) + " neg0:" + Object.is((-0) ** 3, -0));
console.log("neg_zero_even=" + String((-0) ** 2) + " neg0:" + Object.is((-0) ** 2, -0));
console.log("neg_zero_neg_odd=" + String((-0) ** -3));
console.log("neg_zero_neg_even=" + String((-0) ** -2));

// --- exact fractional exponents ---
console.log("four_half=" + String(4 ** 0.5));
console.log("nine_half=" + String(9 ** 0.5));
console.log("sixteen_quarter=" + String(16 ** 0.25));
console.log("two_half_is_SQRT2=" + (2 ** 0.5 === Math.SQRT2));
console.log("half_neg_one=" + String(0.5 ** -1));
console.log("pow_of_one=" + String(1 ** 1e308) + "," + String(1 ** -1e308));

// --- right associativity, and the operand order of a chain ---
console.log("assoc=" + String(2 ** 3 ** 2) + " vs " + String((2 ** 3) ** 2));
console.log("assoc_matches_right=" + (2 ** 3 ** 2 === 2 ** 9));
console.log("chain_three=" + String(2 ** 2 ** 2 ** 2) + " right_assoc:" + (2 ** 2 ** 2 ** 2 === 65536));

// --- the exponent is coerced, and a huge one saturates ---
console.log("string_exponent=" + String(2 ** ("3" as any)));
console.log("bool_exponent=" + String(2 ** (true as any)));
console.log("huge_exponent=" + String(1.0000001 ** 1e12));
console.log("tiny_base_huge_exponent=" + String(0.9999999 ** 1e12));
