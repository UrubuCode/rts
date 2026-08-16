// Cross-runtime: separators do not alter decimal, binary, octal, or hex literals.
const values = [1_234_567, 0b1010_0101, 0o7_5_5, 0xff_ff, 12.34_56];
console.log(values.join("|"));
console.log(values.reduce((a, b) => a + b, 0));

