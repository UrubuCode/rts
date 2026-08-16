// ONE thing: the special-case tables of the logarithm and exponential family.
// Only the pinned rows (NaN, signed zeros, infinities, exact powers) are here —
// the digits of a general log are implementation-approximated and are not.

function probe(label: string, r: number): void {
  console.log(label + " = " + String(r) + " neg0:" + Object.is(r, -0));
}

// --- Math.log ---
probe("log(NaN)", Math.log(NaN));
probe("log(+0)", Math.log(0));
probe("log(-0)", Math.log(-0));
probe("log(1)", Math.log(1));
probe("log(-1)", Math.log(-1));
probe("log(-Infinity)", Math.log(-Infinity));
probe("log(Infinity)", Math.log(Infinity));
probe("log(-0.5)", Math.log(-0.5));

// --- Math.log2: exact only at exact powers of two ---
probe("log2(NaN)", Math.log2(NaN));
probe("log2(+0)", Math.log2(0));
probe("log2(-0)", Math.log2(-0));
probe("log2(1)", Math.log2(1));
probe("log2(2)", Math.log2(2));
probe("log2(8)", Math.log2(8));
probe("log2(1024)", Math.log2(1024));
probe("log2(2**52)", Math.log2(4503599627370496));
probe("log2(0.5)", Math.log2(0.5));
probe("log2(2**-1074)", Math.log2(Number.MIN_VALUE));
probe("log2(-1)", Math.log2(-1));
probe("log2(Infinity)", Math.log2(Infinity));
probe("log2(-Infinity)", Math.log2(-Infinity));

// --- Math.log10: exact only at exact powers of ten ---
probe("log10(NaN)", Math.log10(NaN));
probe("log10(+0)", Math.log10(0));
probe("log10(-0)", Math.log10(-0));
probe("log10(1)", Math.log10(1));
probe("log10(10)", Math.log10(10));
probe("log10(100)", Math.log10(100));
probe("log10(-1)", Math.log10(-1));
probe("log10(Infinity)", Math.log10(Infinity));
probe("log10(-Infinity)", Math.log10(-Infinity));

// --- Math.log1p: the only member that returns -0 ---
probe("log1p(NaN)", Math.log1p(NaN));
probe("log1p(+0)", Math.log1p(0));
probe("log1p(-0)", Math.log1p(-0));
probe("log1p(-1)", Math.log1p(-1));
probe("log1p(-2)", Math.log1p(-2));
probe("log1p(-1.5)", Math.log1p(-1.5));
probe("log1p(Infinity)", Math.log1p(Infinity));
probe("log1p(-Infinity)", Math.log1p(-Infinity));

// --- Math.exp ---
probe("exp(NaN)", Math.exp(NaN));
probe("exp(+0)", Math.exp(0));
probe("exp(-0)", Math.exp(-0));
probe("exp(Infinity)", Math.exp(Infinity));
probe("exp(-Infinity)", Math.exp(-Infinity));
probe("exp(1000)", Math.exp(1000));
probe("exp(-1000)", Math.exp(-1000));

// --- Math.expm1: the mirror of log1p, and it too keeps -0 ---
probe("expm1(NaN)", Math.expm1(NaN));
probe("expm1(+0)", Math.expm1(0));
probe("expm1(-0)", Math.expm1(-0));
probe("expm1(Infinity)", Math.expm1(Infinity));
probe("expm1(-Infinity)", Math.expm1(-Infinity));
probe("expm1(-1000)", Math.expm1(-1000));

// --- Math.sqrt and Math.cbrt keep the sign of zero ---
probe("sqrt(NaN)", Math.sqrt(NaN));
probe("sqrt(+0)", Math.sqrt(0));
probe("sqrt(-0)", Math.sqrt(-0));
probe("sqrt(-1)", Math.sqrt(-1));
probe("sqrt(-Infinity)", Math.sqrt(-Infinity));
probe("sqrt(Infinity)", Math.sqrt(Infinity));
probe("sqrt(2**52)", Math.sqrt(4503599627370496));
probe("cbrt(NaN)", Math.cbrt(NaN));
probe("cbrt(+0)", Math.cbrt(0));
probe("cbrt(-0)", Math.cbrt(-0));
probe("cbrt(-8)", Math.cbrt(-8));
probe("cbrt(-27)", Math.cbrt(-27));
probe("cbrt(Infinity)", Math.cbrt(Infinity));
probe("cbrt(-Infinity)", Math.cbrt(-Infinity));

// --- round-trip identities that hold whatever the last digit is ---
console.log("--- identities ---");
console.log("exp_log_1=" + (Math.exp(Math.log(1)) === 1));
console.log("log_exp_0=" + (Math.log(Math.exp(0)) === 0));
console.log("log2_pow2_monotone=" + (Math.log2(4) === 2 && Math.log2(16) === 4));
console.log("sqrt_sq_4=" + (Math.sqrt(4) === 2));
console.log("cbrt_sign=" + (Math.cbrt(-8) === -Math.cbrt(8)));
console.log("log_of_zero_is_neginf=" + (Math.log(0) === -Infinity && Math.log2(0) === -Infinity && Math.log10(0) === -Infinity));

// --- coercion, arity and names ---
console.log("--- shape ---");
console.log("log_no_args=" + String((Math.log as any)()));
console.log("log_null=" + String(Math.log(null as any)));
console.log("log_str=" + String(Math.log("1" as any)));
console.log("exp_arr=" + String(Math.exp([] as any)));
console.log("lengths=" + [Math.log.length, Math.log2.length, Math.exp.length, Math.sqrt.length].join(","));
console.log("names=" + [Math.log1p.name, Math.expm1.name, Math.cbrt.name].join(","));
