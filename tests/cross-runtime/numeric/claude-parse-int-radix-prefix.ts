// parseInt: prefixo 0x, radix, lixo no fim, espacos
console.log(parseInt("0x1F"));        // 31
console.log(parseInt("0x1F", 16));    // 31
console.log(parseInt("1F", 16));      // 31
console.log(parseInt("  42px"));      // 42
console.log(parseInt("08"));          // 8 (nao octal em ES5+)
console.log(parseInt("0o17"));        // 0 (para no 'o')
console.log(parseInt("0b101"));       // 0 (para no 'b')
console.log(parseInt("z", 36));       // 35
console.log(parseInt("10", 2));       // 2
console.log(parseInt("-0xff"));       // -255
console.log(parseInt("123", 0));      // 123 (radix 0 -> 10)
console.log(parseInt("123abc", 10));  // 123
console.log(parseInt("   -  7"));     // NaN
console.log(parseInt("Infinity"));    // NaN
console.log(parseInt("4.9"));         // 4