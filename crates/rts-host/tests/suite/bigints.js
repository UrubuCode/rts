// `BigInt` — a primitive whose digits are on the heap.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("typeof", typeof 1n === "bigint");
check("literal", 1n === 1n);
// The property this exists for: equality is by VALUE. Two bigints computed
// separately live in different slots, and comparing the words would answer
// false — which is the one thing a primitive must not do.
check("computed-equal", (2n + 3n) === 5n);
check("distinct", 1n !== 2n);
// A bigint and a number are never `===`, because their tags differ. No
// conversion is involved and none should be.
check("not-a-number", (1n === 1) === false);

check("add", 1n + 2n === 3n);
check("subtract", 5n - 3n === 2n);
check("multiply", 6n * 7n === 42n);
check("divide", 7n / 2n === 3n);
// Truncation toward zero, not flooring — visible only with a negative operand,
// which is why it is pinned.
check("divide-negative", -7n / 2n === -3n);
check("remainder", 7n % 3n === 1n);
check("remainder-negative", -7n % 3n === -1n);
check("negate", -(5n) === -5n);

// Past 2^53, where a double stops being able to tell two integers apart. This
// is the whole reason the type exists.
check("past-double", 9007199254740993n !== 9007199254740992n);
check("big-product", 4294967296n * 4294967296n === 18446744073709551616n);
check("big-round-trip", BigInt("123456789012345678901234567890") + 1n === 123456789012345678901234567891n);

check("compare-less", 1n < 2n);
check("compare-greater-equal", 2n >= 2n);
// A bigint and a number DO compare — only arithmetic between them is refused —
// and comparing them as doubles would lose every value past 2^53.
check("compare-mixed", 1n < 2);
check("compare-mixed-big", 9007199254740993n > 9007199254740992);

check("bit-and", (12n & 10n) === 8n);
check("bit-or", (12n | 10n) === 14n);
check("bit-xor", (12n ^ 10n) === 6n);
// The two's-complement interpretation of an arbitrary-precision integer, which
// is the genuinely tricky part: `-1n` is all ones, however far you look.
check("bit-and-negative", (-1n & 3n) === 3n);

check("from-number", BigInt(5) === 5n);
check("from-string", BigInt("42") === 42n);
check("from-hex-string", BigInt("0xff") === 255n);
check("from-boolean", BigInt(true) === 1n);
check("from-empty-string", BigInt("") === 0n);
// A non-integer is a `RangeError` in the language; this answers `undefined`.
check("from-fraction", BigInt(1.5) === undefined);

check("to-string", (255n).toString() === "255");
check("to-string-radix", (255n).toString(16) === "ff");
check("to-string-negative", (-5n).toString() === "-5");
check("value-of", (5n).valueOf() === 5n);
check("string-concat", "" + 1n === "1");
check("template-free-concat", 1n + "" === "1");

check("as-int-n", BigInt.asIntN(8, 255n) === -1n);
check("as-uint-n", BigInt.asUintN(8, 255n) === 255n);
check("as-uint-n-wraps", BigInt.asUintN(8, 256n) === 0n);

// Falsiness: `0n` is the only falsy bigint, and whether a bigint is zero is in
// digits the value layer cannot see.
check("zero-is-falsy", !0n);
check("one-is-truthy", !!1n);
check("negative-is-truthy", !!(-1n));

// It is a primitive, so none of what an object does applies.
check("no-properties", (function () {
    let n = 1n;
    n.tag = 5;
    return n.tag === undefined;
})());
check("not-an-object", (1n instanceof Object) === false);

return failed;
