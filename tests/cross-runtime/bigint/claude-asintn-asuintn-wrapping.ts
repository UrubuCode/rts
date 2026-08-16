// ONE thing: BigInt.asIntN / asUintN — modular wrapping at an arbitrary bit
// width, including the degenerate widths 0 and 1 that a naive mask gets wrong.
function row(bits: number, v: bigint) {
  console.log("bits=" + bits + " v=" + v + " int=" + BigInt.asIntN(bits, v) + " uint=" + BigInt.asUintN(bits, v));
}

// Width 0: everything collapses to 0n.
row(0, 0n); row(0, 1n); row(0, -1n); row(0, 255n);

// Width 1: the signed range is {-1, 0}, the unsigned range is {0, 1}.
row(1, 0n); row(1, 1n); row(1, 2n); row(1, 3n); row(1, -1n); row(1, -2n);

// Width 8: the classic byte boundaries.
row(8, 127n); row(8, 128n); row(8, 255n); row(8, 256n); row(8, -1n); row(8, -128n); row(8, -129n);

// Width 32: where an engine backed by i32 would be tempted to shortcut.
row(32, 2147483647n); row(32, 2147483648n); row(32, 4294967295n); row(32, 4294967296n); row(32, -2147483648n);

// Width 64: beyond a double's exact range.
row(64, 9223372036854775807n);
row(64, 9223372036854775808n);
row(64, 18446744073709551615n);
row(64, 18446744073709551616n);
row(64, -9223372036854775808n);
row(64, -9223372036854775809n);

// Width 128 and a value far above it.
row(128, 2n ** 200n);
row(128, -(2n ** 200n));

// Idempotence: wrapping twice at the same width changes nothing.
const w = BigInt.asIntN(16, 70000n);
console.log("idempotent=" + (BigInt.asIntN(16, w) === w));

// The bits argument is coerced with ToIndex.
console.log("floatBits=" + BigInt.asUintN(8.0, 300n));
console.log("strBits=" + BigInt.asUintN("8" as any, 300n));
try { BigInt.asUintN(-1, 1n); } catch (e: any) { console.log("negBits=" + e.constructor.name); }
try { BigInt.asUintN(1.5, 1n); } catch (e: any) { console.log("fracBits=" + e.constructor.name); }
try { (BigInt.asUintN as any)(8, 1); } catch (e: any) { console.log("numberValue=" + e.constructor.name); }

// A string value is coerced to BigInt first.
console.log("strValue=" + BigInt.asUintN(8, "300" as any));

// The result is always a BigInt, never a Number.
console.log("type=" + typeof BigInt.asIntN(8, 1n));
