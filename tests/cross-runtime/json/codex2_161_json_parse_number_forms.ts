// Cross-runtime: JSON numeric grammar parses integers, fractions, and exponents.
const values = JSON.parse("[0,-0,12,-3.5,1e3,2.5E-2]");
console.log(values.map(String).join("|"));
console.log(Object.is(values[1], -0));

