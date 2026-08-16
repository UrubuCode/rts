// ONE thing: the pinned rows of the circular trigonometric functions — which
// of them return NaN outside their domain, which preserve negative zero, and
// that an infinite argument is a domain error for sin/cos/tan but not for atan.

function probe(label: string, r: number): void {
  console.log(label + " = " + String(r) + " neg0:" + Object.is(r, -0));
}

// --- sin: keeps the sign of zero, NaN at infinity ---
probe("sin(NaN)", Math.sin(NaN));
probe("sin(+0)", Math.sin(0));
probe("sin(-0)", Math.sin(-0));
probe("sin(Infinity)", Math.sin(Infinity));
probe("sin(-Infinity)", Math.sin(-Infinity));

// --- cos: 1 at both zeros, NaN at infinity ---
probe("cos(NaN)", Math.cos(NaN));
probe("cos(+0)", Math.cos(0));
probe("cos(-0)", Math.cos(-0));
probe("cos(Infinity)", Math.cos(Infinity));
probe("cos(-Infinity)", Math.cos(-Infinity));

// --- tan: like sin at the edges ---
probe("tan(NaN)", Math.tan(NaN));
probe("tan(+0)", Math.tan(0));
probe("tan(-0)", Math.tan(-0));
probe("tan(Infinity)", Math.tan(Infinity));
probe("tan(-Infinity)", Math.tan(-Infinity));

// --- asin: domain [-1,1], keeps the sign of zero ---
probe("asin(NaN)", Math.asin(NaN));
probe("asin(+0)", Math.asin(0));
probe("asin(-0)", Math.asin(-0));
probe("asin(2)", Math.asin(2));
probe("asin(-2)", Math.asin(-2));
probe("asin(1.0000001)", Math.asin(1.0000001));
probe("asin(Infinity)", Math.asin(Infinity));
probe("asin(-Infinity)", Math.asin(-Infinity));

// --- acos: domain [-1,1], and exactly +0 at 1 ---
probe("acos(NaN)", Math.acos(NaN));
probe("acos(1)", Math.acos(1));
probe("acos(2)", Math.acos(2));
probe("acos(-2)", Math.acos(-2));
probe("acos(1.0000001)", Math.acos(1.0000001));
probe("acos(Infinity)", Math.acos(Infinity));

// --- atan: total domain, keeps the sign of zero, finite at infinity ---
probe("atan(NaN)", Math.atan(NaN));
probe("atan(+0)", Math.atan(0));
probe("atan(-0)", Math.atan(-0));

// --- the pi-family answers, as identities rather than digits ---
console.log("--- pi identities ---");
const HALF = Math.PI / 2;
console.log("atan_inf_is_half=" + (Math.atan(Infinity) === HALF));
console.log("atan_neginf_is_neghalf=" + (Math.atan(-Infinity) === -HALF));
console.log("asin_1_is_half=" + (Math.asin(1) === HALF));
console.log("asin_neg1_is_neghalf=" + (Math.asin(-1) === -HALF));
console.log("acos_neg1_is_pi=" + (Math.acos(-1) === Math.PI));
console.log("acos_0_is_half=" + (Math.acos(0) === HALF));
console.log("acos_neg0_is_half=" + (Math.acos(-0) === HALF));

// --- parity and range, which hold at every precision ---
console.log("--- parity and range ---");
console.log("sin_odd=" + (Math.sin(-1) === -Math.sin(1)));
console.log("tan_odd=" + (Math.tan(-1) === -Math.tan(1)));
console.log("cos_even=" + (Math.cos(-1) === Math.cos(1)));
console.log("asin_odd=" + (Math.asin(-0.5) === -Math.asin(0.5)));
console.log("atan_odd=" + (Math.atan(-3) === -Math.atan(3)));
console.log("sin_bounded=" + (Math.sin(2) <= 1 && Math.sin(2) >= -1));
console.log("cos_bounded=" + (Math.cos(2) <= 1 && Math.cos(2) >= -1));
console.log("atan_bounded=" + (Math.atan(1e300) <= HALF && Math.atan(-1e300) >= -HALF));
console.log("acos_range=" + (Math.acos(0.3) >= 0 && Math.acos(0.3) <= Math.PI));

// --- the constants are exactly the doubles the spec names, and frozen ---
console.log("--- constants ---");
console.log("PI=" + Math.PI);
console.log("E=" + Math.E);
console.log("LN2=" + Math.LN2);
console.log("LN10=" + Math.LN10);
console.log("LOG2E=" + Math.LOG2E);
console.log("LOG10E=" + Math.LOG10E);
console.log("SQRT2=" + Math.SQRT2);
console.log("SQRT1_2=" + Math.SQRT1_2);
const desc = Object.getOwnPropertyDescriptor(Math, "PI") as any;
console.log("PI_flags=" + [desc.writable, desc.enumerable, desc.configurable].join(","));
console.log("Math_tag=" + Object.prototype.toString.call(Math));
console.log("Math_toStringTag=" + (Math as any)[Symbol.toStringTag]);

// --- coercion and shape ---
console.log("--- shape ---");
console.log("sin_no_args=" + String((Math.sin as any)()));
console.log("sin_null=" + String(Math.sin(null as any)));
console.log("sin_null_neg0=" + Object.is(Math.sin(null as any), -0));
console.log("cos_emptyarr=" + String(Math.cos([] as any)));
console.log("tan_obj=" + String(Math.tan({} as any)));
console.log("asin_str=" + String(Math.asin("0" as any)));
console.log("lengths=" + [Math.sin.length, Math.cos.length, Math.tan.length, Math.asin.length, Math.acos.length, Math.atan.length].join(","));
