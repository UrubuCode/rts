// Cross-runtime: coerção de null/undefined no operador `+`.
// Bug RTS — `1 + null + undefined` deve ser número: 1 + null(→0) = 1, depois
// 1 + undefined(→NaN) = NaN. O RTS trata null/undefined como string nesse
// contexto, produzindo "1nullundefined". Sem nenhum operando string, `+` é
// adição numérica (ToNumber), não concatenação. Bun/Node: NaN.
console.log(1 + null + undefined);
console.log(1 + null);          // 1
console.log(1 + undefined);     // NaN
console.log(null + null);       // 0
console.log(undefined + 1);     // NaN
console.log(true + 1);          // 2 (true→1)
console.log(false + null);      // 0
