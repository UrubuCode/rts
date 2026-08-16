// Cross-runtime: Number coercion handles whitespace and radix-prefixed strings.
const inputs = ["  ", "\t42\n", "0b1011", "0o17", "0x1f", ""];
console.log(inputs.map(Number).join("|"));
console.log(Number("1_000"), Number("+0x10"));

