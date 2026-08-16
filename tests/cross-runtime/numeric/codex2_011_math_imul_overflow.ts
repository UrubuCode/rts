// Cross-runtime: imul performs signed 32-bit multiplication.
console.log(Math.imul(0xffffffff, 5));
console.log(Math.imul(0x7fffffff, 2), Math.imul(0x80000000, 2));
console.log(Math.imul(0x12345678, 0x9abcdef0));

