// Cross-runtime: precisão de inteiro acima de Number.MAX_SAFE_INTEGER.
// Bug RTS — números JS são f64 (double); acima de 2^53 a precisão é de 2 em 2.
// `9007199254740991 + 2` deve dar 9007199254740992 (não ...993, que é
// inexato num double e arredonda para o par). O RTS parece tratar como inteiro
// exato (i64), divergindo da semântica f64 do JS. Relaciona-se a #305.
// Bun/Node: 992 / 992.
console.log(9007199254740991 + 1);   // 9007199254740992
console.log(9007199254740991 + 2);   // 9007199254740992 (não 993!)
console.log(9007199254740993);       // 9007199254740992 (literal já arredonda)
console.log(0.1 + 0.2);              // 0.30000000000000004
console.log(0.1 + 0.2 === 0.3);     // false
