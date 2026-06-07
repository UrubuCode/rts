// Math.round: -0.5 -> -0 (round-half-up para +Inf), trunc, sign
console.log(Math.round(-0.5));            // -0
console.log(Object.is(Math.round(-0.5), -0)); // true
console.log(Math.round(0.5));             // 1
console.log(Math.round(-1.5));            // -1
console.log(Math.round(2.5));             // 3
console.log(Math.round(-2.5));            // -2
console.log(Math.round(-0.4));            // -0
console.log(Object.is(Math.round(-0.1), -0)); // true
console.log(Math.trunc(-4.7));            // -4
console.log(Math.trunc(4.7));             // 4
console.log(Object.is(Math.trunc(-0.2), -0)); // true
console.log(Math.sign(-3));               // -1
console.log(Math.sign(0));                // 0
console.log(Object.is(Math.sign(-0), -0)); // true
console.log(Math.sign(NaN));             // NaN