// Precisao acima de 2^53: inteiros consecutivos colidem
console.log(Number.MAX_SAFE_INTEGER);          // 9007199254740991
console.log(9007199254740992 === 9007199254740993); // true (colisao)
console.log(9007199254740993);                 // 9007199254740992
console.log(2 ** 53);                           // 9007199254740992
console.log(2 ** 53 + 1);                       // 9007199254740992
console.log(2 ** 53 + 2);                       // 9007199254740994
console.log(Number.isSafeInteger(2 ** 53));     // false
console.log(Number.isSafeInteger(2 ** 53 - 1)); // true
console.log(18014398509481984 + 1);             // 18014398509481984
console.log(9999999999999999);                  // 10000000000000000
console.log(123456789012345680 === 123456789012345690); // true