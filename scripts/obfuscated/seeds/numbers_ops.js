// Numeric coercion, bitwise, precedence, comparison chains.
const out = [];
out.push(0.1 + 0.2, (0.1 + 0.2).toFixed(2));
out.push(1 / 0, -1 / 0, 0 / 0);
out.push(5 % 3, -5 % 3, 2 ** 10);
out.push(7 & 3, 7 | 8, 7 ^ 5, ~7, 1 << 5, -16 >> 2, -16 >>> 28);
out.push("5" * "2", "5" + 2, 5 + "2", +"3.5", -"2");
out.push([] + [], [] + {}, 1 + null, 1 + undefined);
out.push(null == undefined, null === undefined, NaN === NaN);
out.push(Math.max(1, 9, 3), Math.min(...[4, 2, 8]));
out.push((255).toString(16), parseInt("ff", 16), Number("0b101"));
out.push(Number.isInteger(5.0), Number.MAX_SAFE_INTEGER);
let i = 5;
out.push(i++ + ++i, i);
console.log(out.join("|"));
