// ONE thing: BigInt division truncates toward ZERO (so the remainder follows
// the dividend), a zero divisor is a RangeError rather than an Infinity, and
// the bitwise operators act on an infinite two's-complement representation —
// which makes >> on a negative value floor, not truncate.

function op(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- division truncates toward zero in all four sign combinations ---
op("div_pp", () => 7n / 2n);
op("div_np", () => -7n / 2n);
op("div_pn", () => 7n / -2n);
op("div_nn", () => -7n / -2n);
op("div_exact", () => 6n / 3n);
op("div_smaller", () => 1n / 2n);
op("div_smaller_neg", () => -1n / 2n);
op("div_huge", () => (10n ** 30n) / 7n);

// --- the remainder takes the sign of the dividend, never the divisor ---
op("mod_pp", () => 7n % 2n);
op("mod_np", () => -7n % 2n);
op("mod_pn", () => 7n % -2n);
op("mod_nn", () => -7n % -2n);
op("mod_exact", () => 6n % 3n);
op("mod_smaller", () => 1n % 2n);
op("identity", () => (-7n / 2n) * 2n + (-7n % 2n));

// --- there is no Infinity to fall back on ---
op("div_by_zero", () => 1n / 0n);
op("mod_by_zero", () => 1n % 0n);
op("zero_div_zero", () => 0n / 0n);
op("zero_div_one", () => 0n / 1n);
op("neg_zero_literal", () => -0n);
op("neg_zero_is_zero", () => Object.is(-0n, 0n));

// --- exponentiation refuses a negative exponent instead of producing 0 ---
op("pow", () => 2n ** 10n);
op("pow_zero", () => 0n ** 0n);
op("pow_neg_base_odd", () => (-2n) ** 3n);
op("pow_neg_base_even", () => (-2n) ** 4n);
op("pow_neg_exp", () => 2n ** -1n);
op("pow_zero_neg_exp", () => 0n ** -1n);
op("pow_one_neg_exp", () => 1n ** -1n);
op("pow_big", () => 2n ** 64n);

// --- shifts: negative counts reverse the direction ---
op("shl", () => 1n << 8n);
op("shl_big", () => 1n << 100n);
op("shl_neg_count", () => 8n << -2n);
op("shr", () => 256n >> 4n);
op("shr_neg_count", () => 1n >> -8n);
op("shr_to_zero", () => 1n >> 1n);
op("shr_neg_value", () => -1n >> 1n);
op("shr_neg_value2", () => -5n >> 1n);
op("shr_neg_far", () => -5n >> 100n);
op("shr_pos_far", () => 5n >> 100n);
op("shl_neg_value", () => -1n << 3n);
op("ushr", () => 8n >>> 1n);

// --- bitwise over an infinite two's complement ---
op("not_5", () => ~5n);
op("not_neg1", () => ~-1n);
op("not_0", () => ~0n);
op("and_neg", () => -1n & 255n);
op("and_neg_neg", () => -1n & -2n);
op("or_neg", () => -2n | 1n);
op("xor_self_neg", () => -1n ^ -1n);
op("xor_neg", () => -1n ^ 5n);
op("and_big", () => (2n ** 100n) & 1n);
op("or_big", () => (2n ** 100n) | 1n);

// --- asIntN / asUintN at the degenerate widths 0 and 1 ---
op("asIntN_0", () => BigInt.asIntN(0, 5n));
op("asUintN_0", () => BigInt.asUintN(0, -5n));
op("asIntN_1_of_1", () => BigInt.asIntN(1, 1n));
op("asIntN_1_of_0", () => BigInt.asIntN(1, 0n));
op("asIntN_1_of_neg1", () => BigInt.asIntN(1, -1n));
op("asUintN_1_of_3", () => BigInt.asUintN(1, 3n));
op("asUintN_1_of_neg1", () => BigInt.asUintN(1, -1n));
op("asIntN_3_of_7", () => BigInt.asIntN(3, 7n));
op("asIntN_3_of_4", () => BigInt.asIntN(3, 4n));
op("asUintN_3_of_neg1", () => BigInt.asUintN(3, -1n));
op("asIntN_64_wrap", () => BigInt.asIntN(64, 2n ** 63n));
op("asUintN_64_wrap", () => BigInt.asUintN(64, -1n));
op("asIntN_neg_width", () => BigInt.asIntN(-1, 5n));
op("asIntN_frac_width", () => BigInt.asIntN(3.9, 7n));
op("asIntN_string_width", () => BigInt.asIntN("3" as any, 7n));
op("asIntN_number_value", () => BigInt.asIntN(8, 255 as any));
