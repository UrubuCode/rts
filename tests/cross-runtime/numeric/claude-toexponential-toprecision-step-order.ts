// ONE thing: the STEP ORDER of toExponential / toPrecision versus toFixed.
// toExponential and toPrecision bail out on a non-finite receiver BEFORE the
// range check, so NaN.toExponential(500) is "NaN" while NaN.toFixed(500) throws.

function show(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- the contrast the fixture exists for ---
show("exp_nan_500", () => NaN.toExponential(500));
show("fix_nan_500", () => NaN.toFixed(500));
show("prec_nan_0", () => NaN.toPrecision(0));
show("fix_nan_neg1", () => NaN.toFixed(-1));
show("exp_inf_neg5", () => Infinity.toExponential(-5));
show("prec_neginf_200", () => (-Infinity).toPrecision(200));
show("exp_neginf_nan_arg", () => (-Infinity).toExponential(NaN));

// --- bounds for a FINITE receiver: toExponential is 0..100 ---
show("exp_0", () => (12345).toExponential(0));
show("exp_100_len", () => String((1).toExponential(100).length));
show("exp_101", () => (1).toExponential(101));
show("exp_neg1", () => (1).toExponential(-1));
show("exp_inf_arg", () => (1).toExponential(Infinity));

// --- bounds for toPrecision are 1..100, so 0 is out ---
show("prec_0", () => (1).toPrecision(0));
show("prec_1", () => (1).toPrecision(1));
show("prec_100_len", () => String((1).toPrecision(100).length));
show("prec_101", () => (1).toPrecision(101));
show("prec_neg1", () => (1).toPrecision(-1));

// --- an omitted argument is not the same as 0 for toExponential ---
show("exp_omitted", () => (123.456).toExponential());
show("exp_undef", () => (123.456).toExponential(undefined));
show("exp_explicit0", () => (123.456).toExponential(0));
show("exp_zero_omitted", () => (0).toExponential());
show("exp_zero_3", () => (0).toExponential(3));
show("exp_negzero_2", () => (-0).toExponential(2));

// --- an omitted argument for toPrecision means "just ToString" ---
show("prec_omitted", () => (123.456).toPrecision());
show("prec_undef", () => (123.456).toPrecision(undefined));
show("prec_e21_omitted", () => (1e21).toPrecision());
show("prec_e21_3", () => (1e21).toPrecision(3));

// --- toPrecision switches to exponential when the exponent leaves -6..p ---
show("prec_1e-6_p1", () => (0.000001).toPrecision(1));
show("prec_1e-7_p1", () => (0.0000001).toPrecision(1));
show("prec_1e-6_p3", () => (0.000001).toPrecision(3));
show("prec_123_p1", () => (123.456).toPrecision(1));
show("prec_123_p2", () => (123.456).toPrecision(2));
show("prec_123_p3", () => (123.456).toPrecision(3));
show("prec_zero_p3", () => (0).toPrecision(3));
show("prec_negzero_p2", () => (-0).toPrecision(2));

// --- the argument is coerced by ToIntegerOrInfinity in both ---
show("exp_str2", () => (12345).toExponential("2" as any));
show("exp_2p9", () => (12345).toExponential(2.9));
show("prec_str3", () => (12345).toPrecision("3" as any));
show("prec_3p9", () => (12345).toPrecision(3.9));
show("prec_valueof", () => (12345).toPrecision({ valueOf: () => 2 } as any));
show("exp_null", () => (12345).toExponential(null as any));
show("prec_null", () => (12345).toPrecision(null as any));
show("prec_true", () => (12345).toPrecision(true as any));
