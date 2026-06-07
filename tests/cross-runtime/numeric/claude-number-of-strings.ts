// Number() coercao de strings: hex, bin, octal literais, vazio, espacos
console.log(Number(""));          // 0
console.log(Number("   "));       // 0
console.log(Number("0x1F"));      // 31
console.log(Number("0b101"));     // 5
console.log(Number("0o17"));      // 15
console.log(Number("  42  "));    // 42
console.log(Number("1e3"));       // 1000
console.log(Number("Infinity"));  // Infinity
console.log(Number("-Infinity")); // -Infinity
console.log(Number("123abc"));    // NaN
console.log(Number("08"));        // 8
console.log(Number(".5"));        // 0.5
console.log(Number("5."));        // 5
console.log(Number(null));        // 0
console.log(Number("\t\n10\n"));  // 10
console.log(Number("+"));         // NaN