// Cross-runtime: clz32 coerces values through unsigned 32-bit representation.
console.log([0, 1, 2, 0xffffffff, -1].map(Math.clz32).join(","));
console.log(Math.clz32(1.9), Math.clz32(0x100000000));

