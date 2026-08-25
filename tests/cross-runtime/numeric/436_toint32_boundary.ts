// ToInt32 across 2^63, which is where the lowering stops being able to convert
// in one instruction.
//
// `19_bitwise_ops.ts` covers the operators; this covers the ONE input range
// they disagree about. `rts-cranelift`'s `Inst::ToInt32` takes a two-instruction
// path when `|x| < 2^63` and the exact sequence otherwise, so every value here
// is a value that decides which. `+Infinity` is the case that made the guard
// necessary: it saturates to `i64::MAX`, whose low 32 bits are `-1`, where the
// language says zero.
const xs: number[] = [
  0, -0, 1, -1, 0.9, -0.9, 1.5, -1.5,
  2147483647, 2147483648, -2147483648, -2147483649,
  4294967295, 4294967296, 4294967297,
  1e21, 1.7976931348623157e308,
  9007199254740991, -9007199254740991,
  9223372036854774784,        // the largest double below 2^63
  9223372036854775808,        // exactly 2^63 — the guard's edge
  -9223372036854775808,
  18446744073709551616,       // 2^64
  1.9342813113834067e25,      // 2^84 — every double here is a multiple of 2^32
  Infinity, -Infinity, NaN,
];
for (const x of xs) {
  console.log(`${x} -> ${x | 0} ${x & 255} ${x >> 1} ${x >>> 1} ${~x} ${(x * 1) | 0}`);
}
