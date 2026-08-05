// `Math` — an object of functions, not a compile-time lowering.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("floor", Math.floor(3.7) === 3);
check("floor-negative", Math.floor(-3.2) === -4);
check("ceil", Math.ceil(3.2) === 4);
check("trunc", Math.trunc(-3.7) === -3);
check("abs", Math.abs(-5) === 5);
check("sqrt", Math.sqrt(16) === 4);
check("cbrt", Math.cbrt(27) === 3);
check("pow", Math.pow(2, 10) === 1024);
check("hypot", Math.hypot(3, 4) === 5);
check("fround", Math.fround(1.5) === 1.5);

// JavaScript rounds a half UP; Rust rounds it away from zero. The pair that
// separates them is a negative half.
check("round-half-up", Math.round(2.5) === 3);
check("round-negative-half", Math.round(-0.5) === 0);
check("round-negative", Math.round(-1.5) === -1);

// `Math.sign` answers the zero itself, where `signum` answers plus or minus one.
check("sign-positive", Math.sign(3) === 1);
check("sign-negative", Math.sign(-3) === -1);
check("sign-zero", Math.sign(0) === 0);
check("sign-nan", isNaN(Math.sign(0 / 0)));

// The identity is not zero, and `NaN` propagates — the two things `f64::max`
// gets wrong.
check("max", Math.max(1, 2) === 2);
check("max-one", Math.max(1) === 1);
check("max-nan", isNaN(Math.max(0 / 0, 1)));
check("min", Math.min(3, 2) === 2);

check("pi", Math.PI > 3.14 && Math.PI < 3.15);
check("e", Math.E > 2.71 && Math.E < 2.72);
check("ln2", Math.LN2 > 0.69 && Math.LN2 < 0.70);
check("log2", Math.log2(8) === 3);
check("log10", Math.log10(1000) === 3);
check("exp-zero", Math.exp(0) === 1);
check("sin-zero", Math.sin(0) === 0);
check("cos-zero", Math.cos(0) === 1);
check("atan2", Math.atan2(0, 1) === 0);

// The argument goes through ToNumber once, in the generated wrapper.
check("coerces", Math.abs("-5") === 5);

// A writable property of a mutable object, which is the whole argument for it
// being an object rather than a lowering.
let original = Math.floor;
Math.floor = function (x) { return 99; };
check("replaceable", Math.floor(3.7) === 99);
Math.floor = original;
check("restored", Math.floor(3.7) === 3);

return failed;
