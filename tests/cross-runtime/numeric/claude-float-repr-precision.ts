// Representacao de floats: shortest round-trip, casos famosos de impressao
console.log(0.1 + 0.2);               // 0.30000000000000004
console.log(0.1 + 0.2 === 0.3);       // false
console.log(0.3 - 0.1);               // 0.19999999999999998
console.log(0.1 * 3);                 // 0.30000000000000004
console.log(1.1 + 2.2);               // 3.3000000000000003
console.log(0.1);                     // 0.1
console.log(100000000000000000000);   // 100000000000000000000
console.log(1000000000000000000000);  // 1e+21
console.log(0.000001);                // 0.000001
console.log(0.0000001);               // 1e-7
console.log(123456789.123456789);     // 123456789.12345679
console.log(5e-324);                  // 5e-324 (menor subnormal)
console.log(Number.EPSILON);          // 2.220446049250313e-16