// ONE thing: the numeric LITERAL grammar. Binary, octal and hex prefixes,
// separators, the exponent forms, a leading dot and a trailing dot all produce
// ordinary doubles — and a separator that a literal accepts is a hard failure
// for Number(), which is a different grammar.

// --- radix prefixes, both cases of the prefix letter and of the digits ---
console.log("bin=" + 0b1010 + "," + 0B1010 + "," + 0b11111111);
console.log("oct=" + 0o17 + "," + 0O17 + "," + 0o777);
console.log("hex=" + 0x1f + "," + 0X1F + "," + 0xff + "," + 0xFF + "," + 0xdeadbeef);
console.log("hex_big=" + 0xffffffff + "," + 0x20000000000000);
console.log("prefix_equalities=" + (0b1111 === 15) + "," + (0o17 === 15) + "," + (0xf === 15));
console.log("prefix_zero=" + 0b0 + "," + 0o0 + "," + 0x0 + "," + Object.is(0x0, -0));

// --- separators are allowed inside every one of those, but never at an edge ---
console.log("sep_dec=" + 1_000_000 + "," + 1_0 + "," + 10_00.00_1);
console.log("sep_bin=" + 0b1010_1010 + "," + 0o1_7 + "," + 0xFF_FF);
console.log("sep_exp=" + 1_0e1_0);
console.log("sep_is_invisible=" + (1_000 === 1000) + "," + (0xFF_FF === 65535));

// --- the exponent forms ---
console.log("exp=" + 1e3 + "," + 1E3 + "," + 1e+3 + "," + 1e-3 + "," + 2.5e2 + "," + 2.5E-2);
console.log("exp_zero=" + 5e0 + "," + 0e0 + "," + 1e0);
console.log("exp_huge=" + 1e308 + "," + 1e309 + "," + 1e-323 + "," + 1e-324);

// --- a leading dot and a trailing dot are both legal ---
console.log("dots=" + .5 + "," + 5. + "," + 5.0 + "," + .25e1);
console.log("dot_equalities=" + (.5 === 0.5) + "," + (5. === 5) + "," + (5.0 === 5));

// --- property access on a numeric literal needs the second dot, or a space ---
console.log("double_dot=" + 5..toString());
console.log("spaced_dot=" + 5 .toString(2));
console.log("parenthesised=" + (5).toString(16));
console.log("decimal_dot=" + 5.0.toString(2));
console.log("fraction_dot=" + 0.5.toString(2));
console.log("exp_dot=" + 5e1.toString(8));
console.log("hex_dot=" + 0xff.toString(36));

// --- every literal is a double: none of these is an integer type ---
console.log("typeof=" + typeof 1 + "," + typeof 0x1f + "," + typeof 1_000 + "," + typeof .5);
console.log("is_integer=" + Number.isInteger(0xff) + "," + Number.isInteger(5.) + "," + Number.isInteger(.5));
console.log("hex_arithmetic=" + (0xff / 2) + "," + (0b1 / 4));
console.log("literal_precision=" + 9007199254740993 + "," + 900719925474099_3);
console.log("long_decimal_literal=" + 0.30000000000000004 + "," + (0.1 + 0.2 === 0.30000000000000004));

// --- the same digits through Number(): the separator is now a failure and the
//     prefixes still work, but a sign in front of a prefix does not ---
const strings: string[] = [
  "1_000", "0b1010", "0o17", "0x1f", "0X1F", "+0x1f", "-0x1f", "1e3", "1E3",
  ".5", "5.", "0b1_0", "0xff", "0o8", "0b2", "1_0e1",
];
for (const s of strings) {
  console.log("Number(" + s + ")=" + String(Number(s)) + " parseInt=" + String(parseInt(s)) + " parseFloat=" + String(parseFloat(s)));
}

// --- BigInt literals take the same prefixes and separators, no dot allowed ---
// (a radix-prefixed BigInt literal is written through String() here on purpose:
// concatenating one DIRECTLY — `"" + 0xffn` — makes the JavaScriptCore-based
// runtime print the source text "0xff" instead of "255", measured. Through
// String(), a variable or
// .toString() every runtime agrees.)
console.log("bigint_forms=" + String(1_000n) + "," + String(0xffn) + "," + String(0b1010n) + "," + String(0o17n) + "," + String(0n));
console.log("bigint_typeof=" + typeof 1n + "," + typeof 0xffn);
console.log("bigint_equalities=" + (0xffn === 255n) + "," + (1_000n === 1000n));

// --- unary minus is an OPERATOR, not part of the literal ---
console.log("neg_literal=" + -0 + "," + Object.is(-0, -0) + "," + Object.is(-(0), -0));
console.log("neg_hex=" + -0xff + "," + (-0xff === -255));
console.log("neg_exp_precedence=" + -1e3 + "," + (0 - 1e3));
