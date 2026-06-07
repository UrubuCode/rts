// Cross-runtime: negative zero propagation through math and stringification.
const z = -0;
console.log(Object.is(z, -0));
console.log(String(z));
console.log(1 / z);
console.log(Object.is(Math.trunc(-0.5), -0));
console.log(Object.is(-0 * 5, -0));
console.log(JSON.stringify([-0, 0]));
