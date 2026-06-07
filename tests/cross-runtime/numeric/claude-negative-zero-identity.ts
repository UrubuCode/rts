// -0 vs +0: Object.is distingue, === nao; 1/-0 = -Infinity
console.log(Object.is(-0, 0));        // false
console.log(Object.is(-0, -0));       // true
console.log(-0 === 0);                 // true
console.log(1 / -0);                   // -Infinity
console.log(1 / 0);                    // Infinity
console.log(1 / (0 * -1));             // -Infinity
console.log(String(-0));              // "0"
console.log((-0).toString());        // "0"
console.log(String(0 / -5));         // "0"
console.log(Object.is(-0, 0 * -1));  // true
console.log(-0 + 0);                  // 0 (positivo)
console.log(Object.is(-0 + 0, 0));   // true