// Operadores bitwise: ToInt32/ToUint32 wraparound em negativos e overflow
console.log(-1 | 0);              // -1
console.log(~0);                  // -1
console.log(~5);                  // -6
console.log(2147483647 + 1 | 0);  // -2147483648 (overflow Int32)
console.log(4294967296 | 0);      // 0 (2^32 -> 0)
console.log(4294967297 | 0);      // 1
console.log(-1 >>> 0);            // 4294967295 (ToUint32)
console.log(3.9 | 0);             // 3 (trunca)
console.log(-3.9 | 0);            // -3
console.log(NaN | 0);            // 0
console.log(Infinity | 0);       // 0
console.log(2147483648 | 0);     // -2147483648
console.log(0xFFFFFFFF | 0);     // -1
console.log(1e10 & 0xFF);        // 168 (1e10 mod 2^32 -> ...)
console.log(-2147483649 | 0);    // 2147483647