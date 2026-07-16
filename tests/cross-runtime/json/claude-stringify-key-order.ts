// Cross-runtime: ordem das chaves no JSON.stringify.
// Regra: indices de array inteiro primeiro, em ordem numerica crescente;
// depois chaves string em ordem de insercao.

// --- insercao pura (sem indices)
const a: any = {};
a.zeta = 1;
a.alpha = 2;
a.mid = 3;
console.log("insertion=" + JSON.stringify(a));

// --- indices inteiros vem antes das strings, ordenados numericamente
const b: any = {};
b.foo = "s";
b["2"] = "two";
b.bar = "s2";
b["1"] = "one";
b["0"] = "zero";
console.log("int_first=" + JSON.stringify(b));

// --- indices desordenados na criacao do literal
console.log("literal=" + JSON.stringify({ "10": "a", "2": "b", "1": "c" }));
console.log("mixed_literal=" + JSON.stringify({ b: 1, "2": 2, a: 3, "1": 4 }));

// --- ordem numerica, nao lexicografica (10 depois de 9)
const c: any = {};
c["10"] = "ten";
c["9"] = "nine";
c["100"] = "hundred";
console.log("numeric_order=" + JSON.stringify(c));

// --- o que NAO conta como indice de array
const d: any = {};
d["01"] = "leading_zero";
d["1"] = "one";
d["-1"] = "negative";
d["1.5"] = "float";
d["+1"] = "plus";
d[""] = "empty";
console.log("not_index=" + JSON.stringify(d));

// --- limite do indice de array (2^32-2 e indice; 2^32-1 nao)
const e: any = {};
e["4294967295"] = "not_index";
e["4294967294"] = "max_index";
e.tail = "t";
console.log("index_limit=" + JSON.stringify(e));

// --- delete e re-insercao move a chave pro fim
const f: any = { x: 1, y: 2, z: 3 };
delete f.x;
f.x = 4;
console.log("reinsert=" + JSON.stringify(f));

// --- chaves numericas aninhadas
console.log("nested=" + JSON.stringify({ outer: { b: 1, "0": 2 } }));

// --- array preserva ordem posicional
console.log("array=" + JSON.stringify(["a", "b", "c"]));

// --- Object.keys concorda com a ordem do stringify
console.log("keys_agree=" + Object.keys(b).join(","));
