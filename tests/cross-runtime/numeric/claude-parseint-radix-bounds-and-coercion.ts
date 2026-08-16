// ONE thing: how parseInt treats its SECOND argument. The radix goes through
// ToInt32 (so it wraps at 2^32 and truncates fractions), 0 means "auto", and
// anything outside 2..36 after that gives NaN before a single digit is read.

function show(label: string, v: number): void {
  console.log(label + "=" + String(v));
}

// --- ToInt32 on the radix: truncation, wrapping, and string coercion ---
show("r2", parseInt("11", 2));
show("r2p9", parseInt("11", 2.9));
show("r2p1", parseInt("11", 2.1));
show("r_str2", parseInt("11", "2" as any));
show("r_str2p9", parseInt("11", "2.9" as any));
show("r_valueof16", parseInt("11", { valueOf: () => 16 } as any));
show("r_array16", parseInt("11", [16] as any));
show("r_wrap_2pow32_plus2", parseInt("11", 4294967298));
show("r_wrap_2pow32_plus16", parseInt("11", 4294967312));
show("r_wrap_exact_2pow32", parseInt("11", 4294967296));
show("r_neg_wrap", parseInt("11", -4294967294));

// --- the values that mean "no radix given" ---
show("r0", parseInt("11", 0));
show("r_undef", parseInt("11", undefined));
show("r_null", parseInt("11", null as any));
show("r_nan", parseInt("11", NaN));
show("r_inf", parseInt("11", Infinity));
show("r_neginf", parseInt("11", -Infinity));
show("r_false", parseInt("11", false as any));
show("r_empty_str", parseInt("11", "" as any));
show("r_object", parseInt("11", {} as any));
show("r_omitted", parseInt("11"));

// --- outside 2..36 the answer is NaN, whatever the digits say ---
show("r1", parseInt("11", 1));
show("r_true_is_1", parseInt("11", true as any));
show("r37", parseInt("11", 37));
show("r36", parseInt("11", 36));
show("r_neg2", parseInt("11", -2));
show("r_neg16", parseInt("ff", -16));
show("r36_zz", parseInt("zz", 36));
show("r36_z", parseInt("z", 36));
show("r35_z", parseInt("z", 35));
show("r2_digit2", parseInt("2", 2));
show("r8_digit8", parseInt("8", 8));

// --- the 0x prefix is stripped only when the radix is 0 or 16 ---
show("hex_r0", parseInt("0x11", 0));
show("hex_r16", parseInt("0x11", 16));
show("hex_r16_upper", parseInt("0X11", 16));
show("hex_r10", parseInt("0x11", 10));
show("hex_r8", parseInt("0x11", 8));
show("hex_r36", parseInt("0x11", 36));
show("hex_neg_r16", parseInt("-0x1f", 16));
show("hex_neg_r0", parseInt("-0x1f", 0));
show("hex_plus_r16", parseInt("+0x1f", 16));
show("bare_x_r16", parseInt("0x", 16));
show("bare_x_r0", parseInt("0x", 0));
show("hex_r34", parseInt("0x11", 34));

// --- parseInt reads a prefix; a bad radix digit stops it where it stands ---
show("stop_r2", parseInt("10201", 2));
show("stop_r8", parseInt("1789", 8));
show("stop_r16", parseInt("1fg2", 16));
show("empty_prefix_r16", parseInt("g2", 16));

// --- parseInt("-0") keeps the sign of zero; parseInt("0") does not ---
console.log("neg0_isNeg0=" + Object.is(parseInt("-0"), -0));
console.log("pos0_isNeg0=" + Object.is(parseInt("0"), -0));
console.log("neg0_r16_isNeg0=" + Object.is(parseInt("-0", 16), -0));
console.log("parsefloat_neg0_isNeg0=" + Object.is(parseFloat("-0"), -0));
