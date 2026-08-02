// Cross-runtime: um método que NÃO existe na superfície de Number.
//
// Chamar `x.naoExiste()` num number é TypeError em RUNTIME, no ponto da
// chamada — não erro de compilação. O RTS recusava o ARQUIVO INTEIRO
// ("no Registry entry for `Number.<m>`").
//
// O caso real não é nem um number: todo HANDLE de runtime (Promise pendente,
// nó de DOM, socket) viaja pela superfície NUMBER, então
// `elemento.getAttribute("x")` chegava como `Number.getAttribute` e derrubava o
// bundle. Agora um método ausente em Number cai no despacho dinâmico — que lê o
// método do próprio receiver —, e só um miss de verdade vira TypeError.

let tipo = "NAO-LANCOU";
try {
  (5 as any).metodoInexistente();
} catch (e) {
  tipo = (e as Error).constructor.name;
}
console.log("ausente=" + tipo);

let tipoComArgs = "NAO-LANCOU";
try {
  (7 as any).outroInexistente(1, 2);
} catch (e) {
  tipoComArgs = (e as Error).constructor.name;
}
console.log("ausente_com_args=" + tipoComArgs);

// a superfície REAL de Number não pode ter regredido
console.log("toFixed=" + (3.14159).toFixed(2));
console.log("toString_16=" + (255).toString(16));
console.log("toString_2=" + (5).toString(2));
console.log("valueOf=" + (42).valueOf());
console.log("toPrecision=" + (123.456).toPrecision(4));
console.log("toExponential=" + (1234).toExponential(2));
console.log("toLocaleString_existe=" + (typeof (1).toLocaleString === "function"));
