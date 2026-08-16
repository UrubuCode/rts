// ONE thing: exactly where mixing BigInt with Number throws. Arithmetic and
// bitwise operators refuse the mix; relational and loose-equality operators
// accept it; unary + and >>> refuse BigInt outright, even BigInt alone.

const b: any = 2n;
const n: any = 3;

function op(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- arithmetic refuses the mix in either order ---
op("bigint_plus_number", () => b + n);
op("number_plus_bigint", () => n + b);
op("bigint_minus_number", () => b - n);
op("bigint_times_number", () => b * n);
op("bigint_div_number", () => b / n);
op("bigint_mod_number", () => b % n);
op("bigint_pow_number", () => b ** n);
op("number_pow_bigint", () => n ** b);
op("bigint_plus_bool", () => b + (true as any));
op("bigint_plus_null", () => b + (null as any));
op("bigint_plus_undefined", () => b + (undefined as any));
op("bigint_plus_nan", () => b + (NaN as any));

// --- but BigInt with BigInt is fine, and so is unary minus ---
op("bigint_plus_bigint", () => b + 3n);
op("unary_minus", () => -b);
op("unary_tilde", () => ~b);
op("unary_plus", () => +b);
op("unary_plus_wrapper", () => +(Object(1n) as any));

// --- bitwise operators refuse the mix too, and >>> refuses BigInt entirely ---
op("and_mixed", () => b & n);
op("or_mixed", () => b | n);
op("xor_mixed", () => b ^ n);
op("shl_mixed", () => b << n);
op("shr_mixed", () => b >> n);
op("and_pure", () => b & 3n);
op("shl_pure", () => b << 3n);
op("ushr_pure", () => b >>> 1n);
op("ushr_mixed", () => b >>> 1);

// --- + falls back to string concatenation when either side is a string ---
op("bigint_plus_string", () => b + "x");
op("string_plus_bigint", () => "x" + b);
op("bigint_plus_numeric_string", () => b + "3");
op("bigint_minus_string", () => b - ("3" as any));
op("template", () => `${b}`);
op("bigint_plus_array", () => b + ([] as any));
op("bigint_plus_object", () => b + ({} as any));

// --- relational comparison mixes freely, without coercing to a common type ---
op("lt", () => 1n < 2);
op("gt", () => 2n > 1);
op("le", () => 1n <= 1);
op("ge", () => 2n >= 3);
op("lt_float", () => 1n < 1.5);
op("gt_float", () => 2n > 1.5);
op("lt_nan", () => 1n < NaN);
op("gt_nan", () => 1n > NaN);
op("lt_infinity", () => (2n ** 1024n) < Infinity);
op("gt_neginfinity", () => (-(2n ** 1024n)) > -Infinity);
op("lt_string", () => 1n < "2");
op("huge_vs_double", () => (2n ** 53n + 1n) > 9007199254740992);

// --- loose equality crosses the type line; strict equality does not ---
op("loose_eq", () => 1n == 1);
op("loose_eq_float", () => 1n == 1.5);
op("strict_eq", () => 1n === (1 as any));
op("loose_eq_string", () => 1n == "1");
op("loose_eq_hex_string", () => 1n == "0x1");
op("loose_eq_bad_string", () => 1n == "1.5");
op("loose_eq_empty_string", () => 0n == "");
op("loose_eq_bool", () => 0n == false);
op("loose_eq_true", () => 1n == true);
op("loose_eq_null", () => 0n == null);
op("loose_eq_undefined", () => 0n == undefined);
op("loose_eq_nan", () => 0n == NaN);
op("loose_eq_array", () => 0n == ([] as any));
op("loose_eq_wrapper", () => 1n == (Object(1n) as any));

// --- Math refuses BigInt at ToNumber, every entry point alike ---
op("math_abs", () => Math.abs(b));
op("math_max", () => Math.max(b));
op("math_floor", () => Math.floor(b));
op("math_sqrt", () => Math.sqrt(4n as any));
op("math_sign", () => Math.sign(b));

// --- explicit conversion is the way across ---
op("Number_of_bigint", () => Number(2n));
op("String_of_bigint", () => String(2n));
op("Boolean_of_bigint", () => Boolean(0n));
op("BigInt_of_number", () => BigInt(2));
op("isNaN_of_bigint", () => isNaN(b));
op("Number_isInteger", () => Number.isInteger(b));
op("parseInt_of_bigint", () => parseInt(b));
