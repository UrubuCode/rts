// Cross-runtime: bitwise shift counts are masked to 5 bits.
console.log(1 << 32);
console.log(1 << 33);
console.log(-8 >> 33);
console.log(-8 >>> 33);
console.log(0x80000000 >> 31);
console.log(0x80000000 >>> 31);
