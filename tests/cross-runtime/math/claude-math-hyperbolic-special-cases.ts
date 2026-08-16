// ONE thing: the special-case tables of the six hyperbolic functions. Every
// row below is pinned by the spec — the domain edges of acosh and atanh, the
// exact +-1 limits of tanh, and which of them preserve negative zero.

function probe(label: string, r: number): void {
  console.log(label + " = " + String(r) + " neg0:" + Object.is(r, -0));
}

// --- sinh: odd, keeps the sign of zero, unbounded ---
probe("sinh(NaN)", Math.sinh(NaN));
probe("sinh(+0)", Math.sinh(0));
probe("sinh(-0)", Math.sinh(-0));
probe("sinh(Infinity)", Math.sinh(Infinity));
probe("sinh(-Infinity)", Math.sinh(-Infinity));
probe("sinh(1000)", Math.sinh(1000));
probe("sinh(-1000)", Math.sinh(-1000));

// --- cosh: even, 1 at both zeros, never negative ---
probe("cosh(NaN)", Math.cosh(NaN));
probe("cosh(+0)", Math.cosh(0));
probe("cosh(-0)", Math.cosh(-0));
probe("cosh(Infinity)", Math.cosh(Infinity));
probe("cosh(-Infinity)", Math.cosh(-Infinity));
probe("cosh(1000)", Math.cosh(1000));

// --- tanh: saturates at exactly +-1 ---
probe("tanh(NaN)", Math.tanh(NaN));
probe("tanh(+0)", Math.tanh(0));
probe("tanh(-0)", Math.tanh(-0));
probe("tanh(Infinity)", Math.tanh(Infinity));
probe("tanh(-Infinity)", Math.tanh(-Infinity));
probe("tanh(1000)", Math.tanh(1000));
probe("tanh(-1000)", Math.tanh(-1000));
probe("tanh(20)", Math.tanh(20));

// --- asinh: total domain, keeps the sign of zero ---
probe("asinh(NaN)", Math.asinh(NaN));
probe("asinh(+0)", Math.asinh(0));
probe("asinh(-0)", Math.asinh(-0));
probe("asinh(Infinity)", Math.asinh(Infinity));
probe("asinh(-Infinity)", Math.asinh(-Infinity));

// --- acosh: undefined below 1, and exactly +0 at 1 ---
probe("acosh(NaN)", Math.acosh(NaN));
probe("acosh(1)", Math.acosh(1));
probe("acosh(0.99999)", Math.acosh(0.99999));
probe("acosh(0)", Math.acosh(0));
probe("acosh(-0)", Math.acosh(-0));
probe("acosh(-1)", Math.acosh(-1));
probe("acosh(-2)", Math.acosh(-2));
probe("acosh(Infinity)", Math.acosh(Infinity));
probe("acosh(-Infinity)", Math.acosh(-Infinity));

// --- atanh: infinite at +-1, undefined outside, keeps the sign of zero ---
probe("atanh(NaN)", Math.atanh(NaN));
probe("atanh(+0)", Math.atanh(0));
probe("atanh(-0)", Math.atanh(-0));
probe("atanh(1)", Math.atanh(1));
probe("atanh(-1)", Math.atanh(-1));
probe("atanh(2)", Math.atanh(2));
probe("atanh(-2)", Math.atanh(-2));
probe("atanh(1.00001)", Math.atanh(1.00001));
probe("atanh(Infinity)", Math.atanh(Infinity));
probe("atanh(-Infinity)", Math.atanh(-Infinity));

// --- parity identities that hold at every precision ---
console.log("--- parity ---");
console.log("sinh_odd=" + (Math.sinh(-2) === -Math.sinh(2)));
console.log("cosh_even=" + (Math.cosh(-2) === Math.cosh(2)));
console.log("tanh_odd=" + (Math.tanh(-2) === -Math.tanh(2)));
console.log("asinh_odd=" + (Math.asinh(-2) === -Math.asinh(2)));
console.log("atanh_odd=" + (Math.atanh(-0.5) === -Math.atanh(0.5)));
console.log("cosh_ge_1=" + (Math.cosh(0.5) >= 1 && Math.cosh(-3) >= 1));
console.log("tanh_bounded=" + (Math.tanh(5) < 1 && Math.tanh(-5) > -1));
console.log("acosh_nonneg=" + (Math.acosh(2) > 0 && Math.acosh(1) === 0));

// --- coercion and shape ---
console.log("--- shape ---");
console.log("sinh_no_args=" + String((Math.sinh as any)()));
console.log("cosh_null=" + String(Math.cosh(null as any)));
console.log("tanh_str_zero=" + String(Math.tanh("0" as any)));
console.log("atanh_emptyarr=" + String(Math.atanh([] as any)));
console.log("acosh_true=" + String(Math.acosh(true as any)));
console.log("lengths=" + [Math.sinh.length, Math.cosh.length, Math.tanh.length, Math.asinh.length, Math.acosh.length, Math.atanh.length].join(","));
console.log("names=" + [Math.sinh.name, Math.acosh.name, Math.atanh.name].join(","));
