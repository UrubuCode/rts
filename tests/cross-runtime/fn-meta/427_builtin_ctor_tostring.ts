// Cross-runtime: `<ClasseGlobal>.toString()` — um construtor É um objeto-função,
// então isso é `Function.prototype.toString` sobre ele, com a renderização
// `[native code]`. Bundles chamam exatamente isso para detectar um builtin
// adulterado, e o RTS recusava o ARQUIVO INTEIRO por causa dessa sonda.
//
// A GRAFIA EXATA é definida pela implementação e DIVERGE: o V8 (Node) emite uma
// linha só, o JSC (Bun) emite três. Então a fixture normaliza espaços em branco
// e asserta o que a spec de fato garante — o nome da função e o marcador
// `[native code]`. Comparar a string crua faria a fixture falhar em Bun por uma
// diferença que não é defeito de ninguém.

function norma(s: string): string {
  return s.replace(/\s+/g, " ").trim();
}

console.log("String=" + norma(String.toString()));
console.log("Number=" + norma(Number.toString()));
console.log("Object=" + norma(Object.toString()));
console.log("Array=" + norma(Array.toString()));
console.log("Boolean=" + norma(Boolean.toString()));
console.log("Date=" + norma(Date.toString()));

// o idioma como o bundle escreve (sobre o resultado normalizado)
function ehNativo(s: string, nome: string): boolean {
  return norma(s) === "function " + nome + "() { [native code] }";
}
console.log("nativo_String=" + ehNativo(String.toString(), "String"));
console.log("nativo_Array=" + ehNativo(Array.toString(), "Array"));
console.log("nativo_errado=" + ehNativo(Array.toString(), "String"));
