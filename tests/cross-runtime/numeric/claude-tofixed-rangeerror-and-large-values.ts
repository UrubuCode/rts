// ONE thing: the ARGUMENT and RANGE contract of Number.prototype.toFixed.
// The bounds check (f < 0 or f > 100) happens BEFORE the non-finite bail-out
// and before the >= 1e21 fall-back to ToString, so NaN.toFixed(101) throws.

function show(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- the legal bounds are exactly 0..100 inclusive ---
show("f0", () => (1.5).toFixed(0));
show("f1", () => (1.5).toFixed(1));
show("f100_len", () => String((1.5).toFixed(100).length));
show("f101", () => (1.5).toFixed(101));
show("fneg1", () => (1.5).toFixed(-1));
show("fneg0", () => (1.5).toFixed(-0));
show("f1000", () => (1.5).toFixed(1000));

// --- the argument goes through ToIntegerOrInfinity: truncation, not rounding ---
show("f2p9", () => (1.23456).toFixed(2.9));
show("f2p1", () => (1.23456).toFixed(2.1));
show("fstr2", () => (1.23456).toFixed("2" as any));
show("fstr2p9", () => (1.23456).toFixed("2.9" as any));
show("ftrue", () => (1.23456).toFixed(true as any));
show("ffalse", () => (1.23456).toFixed(false as any));
show("fnull", () => (1.23456).toFixed(null as any));
show("fundef", () => (1.23456).toFixed(undefined as any));
show("fnoarg", () => (1.23456).toFixed());
show("fnan", () => (1.23456).toFixed(NaN));
show("fempty_str", () => (1.23456).toFixed("" as any));
show("fvalueof", () => (1.23456).toFixed({ valueOf: () => 3 } as any));
show("farray", () => (1.23456).toFixed([2] as any));

// --- Infinity as the argument is out of range, not "as many as possible" ---
show("finf", () => (1.5).toFixed(Infinity));
show("fneginf", () => (1.5).toFixed(-Infinity));

// --- non-finite receivers ignore the digits, but only after the bounds check ---
show("nan_f2", () => NaN.toFixed(2));
show("nan_f101", () => NaN.toFixed(101));
show("nan_fneg1", () => NaN.toFixed(-1));
show("inf_f2", () => Infinity.toFixed(2));
show("neginf_f0", () => (-Infinity).toFixed(0));
show("inf_f101", () => Infinity.toFixed(101));

// --- at 1e21 the result switches to ToString and the digits are dropped ---
show("e20_f2", () => (1e20).toFixed(2));
show("e21_f2", () => (1e21).toFixed(2));
show("e21_f0", () => (1e21).toFixed(0));
show("e21_neg", () => (-1e21).toFixed(3));
show("e21_5_f4", () => (1.5e21).toFixed(4));
show("e30_f2", () => (1e30).toFixed(2));
show("just_under", () => (999999999999999999999).toFixed(1));
show("e21_f101", () => (1e21).toFixed(101));

// --- signed zero and tiny values are rendered without a sign ---
show("negzero_f2", () => (-0).toFixed(2));
show("negtiny_f2", () => (-0.0001).toFixed(2));
show("negtiny_f5", () => (-0.0001).toFixed(5));
show("minvalue_f100_tail", () => Number.MIN_VALUE.toFixed(100).slice(-4));
show("zero_f100_len", () => String((0).toFixed(100).length));
