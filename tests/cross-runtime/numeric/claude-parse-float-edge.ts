// parseFloat: notacao cientifica, Infinity, lixo, espacos
console.log(parseFloat("3.14abc"));     // 3.14
console.log(parseFloat("  .5"));        // 0.5
console.log(parseFloat("1e3"));         // 1000
console.log(parseFloat("1.2e-3"));      // 0.0012
console.log(parseFloat("Infinity"));    // Infinity
console.log(parseFloat("-Infinity"));   // -Infinity
console.log(parseFloat("0x10"));        // 0 (para no x)
console.log(parseFloat("."));           // NaN
console.log(parseFloat("5."));          // 5
console.log(parseFloat("3.14.15"));     // 3.14
console.log(parseFloat("1e"));          // 1
console.log(parseFloat("  12  34"));    // 12
console.log(parseFloat("+7.5"));        // 7.5