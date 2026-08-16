// ONE thing: the transcendental identities, asserted as BOOLEANS inside a
// tolerance instead of as digits. The last ulp of exp/log/sin/cos is
// implementation-approximated, so printing the value would pin an engine rather
// than the language; printing "the identity holds to 1e-12" pins the language.

function near(label: string, a: number, b: number): void {
  const diff = Math.abs(a - b);
  const scale = Math.max(1, Math.abs(a), Math.abs(b));
  console.log(label + "=" + (diff / scale < 1e-12));
}

// --- exp and log invert each other ---
near("exp_log_5", Math.exp(Math.log(5)), 5);
near("log_exp_3", Math.log(Math.exp(3)), 3);
near("exp_1_is_E", Math.exp(1), Math.E);
near("log_E", Math.log(Math.E), 1);
near("log2_of_1024", Math.log2(1024), 10);
near("log10_of_100000", Math.log10(100000), 5);
near("log_10_is_LN10", Math.log(10), Math.LN10);
near("log_2_is_LN2", Math.log(2), Math.LN2);
near("log2_e_is_LOG2E", Math.log2(Math.E), Math.LOG2E);
near("log10_e_is_LOG10E", Math.log10(Math.E), Math.LOG10E);

// --- the base-change identity ---
near("log2_via_log", Math.log2(7), Math.log(7) / Math.LN2);
near("log10_via_log", Math.log10(7), Math.log(7) / Math.LN10);

// --- expm1 and log1p are the accurate forms near zero, and still invert ---
near("expm1_log1p", Math.expm1(Math.log1p(0.5)), 0.5);
near("log1p_expm1", Math.log1p(Math.expm1(0.25)), 0.25);
near("expm1_small", Math.expm1(1e-10), 1e-10);
near("log1p_small", Math.log1p(1e-10), 1e-10);

// --- the Pythagorean identity, over a spread of angles ---
const angles: number[] = [0, 0.3, 1, 1.5707963267948966, 2.5, 3, -0.7, -2.2, 10, 100];
for (const t of angles) {
  const s = Math.sin(t);
  const c = Math.cos(t);
  near("sin2_cos2_" + String(t), s * s + c * c, 1);
}

// --- tan is sin/cos, and atan inverts it ---
near("tan_is_sin_over_cos", Math.tan(0.8), Math.sin(0.8) / Math.cos(0.8));
near("atan_tan", Math.atan(Math.tan(0.8)), 0.8);
near("asin_sin", Math.asin(Math.sin(0.4)), 0.4);
near("acos_cos", Math.acos(Math.cos(0.4)), 0.4);
near("atan2_quadrant_one", Math.atan2(1, 1), Math.PI / 4);
near("atan2_matches_atan", Math.atan2(3, 4), Math.atan(3 / 4));

// --- hyperbolic identities ---
near("cosh2_minus_sinh2", Math.cosh(1.3) * Math.cosh(1.3) - Math.sinh(1.3) * Math.sinh(1.3), 1);
near("tanh_is_sinh_over_cosh", Math.tanh(0.9), Math.sinh(0.9) / Math.cosh(0.9));
near("asinh_sinh", Math.asinh(Math.sinh(1.1)), 1.1);
near("acosh_cosh", Math.acosh(Math.cosh(1.1)), 1.1);
near("atanh_tanh", Math.atanh(Math.tanh(0.6)), 0.6);
near("sinh_via_exp", Math.sinh(2), (Math.exp(2) - Math.exp(-2)) / 2);
near("cosh_via_exp", Math.cosh(2), (Math.exp(2) + Math.exp(-2)) / 2);

// --- roots and powers ---
near("sqrt_squared", Math.sqrt(7) * Math.sqrt(7), 7);
near("cbrt_cubed", Math.cbrt(7) * Math.cbrt(7) * Math.cbrt(7), 7);
near("cbrt_negative", Math.cbrt(-7), -Math.cbrt(7));
near("pow_half_is_sqrt", Math.pow(7, 0.5), Math.sqrt(7));
near("pow_third_is_cbrt", Math.pow(7, 1 / 3), Math.cbrt(7));
near("hypot_is_sqrt_sum", Math.hypot(3.5, 4.5), Math.sqrt(3.5 * 3.5 + 4.5 * 4.5));
near("sqrt2_constant", Math.sqrt(2), Math.SQRT2);
near("sqrt_half_constant", Math.sqrt(0.5), Math.SQRT1_2);

// --- the constants relate to each other exactly enough for the tolerance ---
near("ln2_times_log2e", Math.LN2 * Math.LOG2E, 1);
near("ln10_times_log10e", Math.LN10 * Math.LOG10E, 1);
near("sqrt2_times_sqrt1_2", Math.SQRT2 * Math.SQRT1_2, 1);
near("sin_pi_is_zero_ish", Math.sin(Math.PI) + 1, 1);
