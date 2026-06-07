// Math.ceil/floor preservam -0; modulo % e sinal; expoente fracionario
console.log(Object.is(Math.ceil(-0.5), -0)); // true
console.log(Math.ceil(-0.5));          // -0
console.log(Math.floor(-0.0));         // -0
console.log(Object.is(Math.floor(0), 0)); // true
console.log(-5 % 3);                   // -2 (sinal do dividendo)
console.log(5 % -3);                   // 2
console.log(Object.is(-0 % 5, -0));    // true
console.log(-0 % 5);                   // -0
console.log(0 % -5);                   // 0
console.log(5.5 % 2);                  // 1.5
console.log(Math.pow(-8, 1 / 3));      // NaN (raiz nao-inteira de negativo)
console.log(Math.cbrt(-8));            // -2
console.log(2 ** -1);                  // 0.5
console.log((-2) ** 2);                // 4
console.log(Math.ceil(2.0001));        // 3