// Cross-runtime: `Object.fromEntries` sobre uma fonte NÃO-array.
//
// O RTS só aceitava a fonte quando conseguia provar estaticamente que era um
// array (ou uma classe conhecida com `entries()`); uma fonte opaca — um Map
// numa variável `any`, ou um parâmetro, que é o que código minificado passa —
// recusava o ARQUIVO INTEIRO. Agora o trampolim resolve `entries()` por NOME em
// runtime, então a prova estática deixa de ser necessária.

const m = new Map<string, number>([["a", 1], ["b", 2]]);
console.log("map=" + JSON.stringify(Object.fromEntries(m)));
console.log("array=" + JSON.stringify(Object.fromEntries([["x", 9]])));
console.log("vazio=" + JSON.stringify(Object.fromEntries(new Map())));

// fonte OPACA (o caso que bailava): o tipo não é visível no ponto da chamada
function viaParam(src: any): any {
  return Object.fromEntries(src);
}
console.log("opaco_map=" + JSON.stringify(viaParam(m)));
console.log("opaco_array=" + JSON.stringify(viaParam([["y", 7]])));

// ida e volta
console.log("roundtrip=" + JSON.stringify(Object.fromEntries(Object.entries({ k: 3 }))));

// chaves numéricas viram string, como manda a spec
console.log("chave_num=" + JSON.stringify(Object.fromEntries([[1, "um"]])));

// a última ocorrência de uma chave repetida vence
console.log("dup=" + JSON.stringify(Object.fromEntries([["d", 1], ["d", 2]])));
