// Cross-runtime: shift counts are masked to five bits.
console.log([1 << 0, 1 << 31, 1 << 32, 1 << 33].join("|"));
console.log([-1 >> 1, -1 >>> 1, 0x80000000 >> 31].join("|"));

