// ONE thing: what BigInt() accepts. Strings go through StringToBigInt, which
// is the numeric-literal grammar MINUS decimals, exponents and separators, and
// which allows a sign only in front of a DECIMAL literal — so "-0x1f" fails.
// A non-integral Number is a RangeError, not a truncation.

function big(label: string, v: any): void {
  try {
    console.log(label + "=" + String(BigInt(v)) + "n");
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- decimal strings, with whitespace trimmed from both ends ---
big("plain", "10");
big("padded", "  10  ");
big("tabbed", "\t10\n");
big("empty", "");
big("blank", "   ");
big("nbsp_only", " ");
big("bom_wrapped", "﻿10﻿");
big("plus", "+10");
big("minus", "-10");
big("minus_zero", "-0");
big("plus_zero", "+0");
big("leading_zeros", "007");
big("inner_space", "1 0");
big("trailing_junk", "10x");
big("long", "123456789012345678901234567890");

// --- non-decimal literals are accepted, but never with a sign ---
big("hex_lower", "0x1f");
big("hex_upper", "0X1F");
big("binary", "0b101");
big("binary_upper", "0B101");
big("octal", "0o17");
big("octal_upper", "0O17");
big("hex_signed", "-0x1f");
big("hex_plus", "+0x1f");
big("bare_0x", "0x");
big("legacy_octal", "017");

// --- what the numeric grammar has that BigInt does not ---
big("fraction", "1.5");
big("trailing_dot", "1.");
big("leading_dot", ".5");
big("exponent", "1e3");
big("exponent_upper", "1E3");
big("separator", "1_0");
big("bigint_suffix", "1n");
big("infinity", "Infinity");
big("neg_infinity", "-Infinity");
big("nan_string", "NaN");

// --- Numbers must be integral and finite ---
big("num_int", 10);
big("num_neg", -10);
big("num_zero", 0);
big("num_negzero", -0);
big("num_fraction", 1.5);
big("num_tiny_fraction", 1.0000000000000002);
big("num_nan", NaN);
big("num_infinity", Infinity);
big("num_neg_infinity", -Infinity);
big("num_2pow53", 9007199254740992);
big("num_2pow53_plus1_literal", 9007199254740993);
big("num_1e21", 1e21);
big("num_max_value", Number.MAX_VALUE);
big("num_min_value", Number.MIN_VALUE);

// --- other primitive types ---
big("true", true);
big("false", false);
big("null", null);
big("undefined", undefined);
big("symbol", Symbol("s"));
big("bigint_itself", 7n);

// --- objects go through ToPrimitive with hint "number" ---
big("empty_array", []);
big("array_one", [15]);
big("array_two", [1, 2]);
big("array_nested", [[7]]);
big("plain_object", {});
big("valueof_num", { valueOf: function () { return 7; } });
big("valueof_frac", { valueOf: function () { return 7.5; } });
big("valueof_string", { valueOf: function () { return "0x10"; } });
big("tostring_only", { toString: function () { return "42"; } });
big("number_wrapper", new Number(9));
big("string_wrapper", new String("9"));
big("bigint_wrapper", Object(9n));

// --- BigInt is not a constructor ---
try {
  const made = new (BigInt as any)(1);
  console.log("new_BigInt=" + String(made));
} catch (e) {
  console.log("new_BigInt!" + (e as any).constructor.name);
}
console.log("BigInt_length=" + BigInt.length);
console.log("BigInt_name=" + BigInt.name);
console.log("has_prototype=" + (typeof BigInt.prototype));
console.log("proto_tag=" + (BigInt.prototype as any)[Symbol.toStringTag]);
console.log("asIntN_length=" + BigInt.asIntN.length);
console.log("asUintN_length=" + BigInt.asUintN.length);
