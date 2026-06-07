// >>> (unsigned) vs >> (signed) e shift count mod 32
console.log(-1 >>> 0);     // 4294967295
console.log(-1 >>> 1);     // 2147483647
console.log(-8 >>> 2);     // 1073741822
console.log(-8 >> 2);      // -2
console.log(1 << 31);      // -2147483648
console.log(1 << 32);      // 1 (shift count mod 32 = 0)
console.log(1 << 33);      // 2
console.log(256 >>> 40);   // 1 (40 mod 32 = 8)
console.log(-2147483648 >>> 0); // 2147483648
console.log(0xFFFFFFFF >>> 0);  // 4294967295
console.log(5 >>> 32);     // 5 (count mod 32 = 0)
console.log(-16 >>> 31);   // 1
console.log(2147483648 >>> 31); // 1