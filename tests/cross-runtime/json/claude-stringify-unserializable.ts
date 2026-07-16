// Cross-runtime: JSON.stringify de valores nao-serializaveis (undefined/function/symbol).
// Regra: em objeto a chave SOME; em array vira "null"; no topo retorna undefined.
const fn = function () { return 1; };
const sym = Symbol("s");

// --- topo: retorna undefined (nao a string "undefined")
console.log("top_undef=" + String(JSON.stringify(undefined)));
console.log("top_undef_isundef=" + (JSON.stringify(undefined) === undefined));
console.log("top_fn=" + String(JSON.stringify(fn)));
console.log("top_fn_isundef=" + (JSON.stringify(fn) === undefined));
console.log("top_sym=" + String(JSON.stringify(sym)));
console.log("top_sym_isundef=" + (JSON.stringify(sym) === undefined));

// --- em objeto: chave sumida
console.log("obj_undef=" + JSON.stringify({ a: 1, b: undefined, c: 2 }));
console.log("obj_fn=" + JSON.stringify({ a: 1, b: fn, c: 2 }));
console.log("obj_sym=" + JSON.stringify({ a: 1, b: sym, c: 2 }));
console.log("obj_all_gone=" + JSON.stringify({ b: undefined, c: fn, d: sym }));

// --- em array: vira null
console.log("arr_undef=" + JSON.stringify([1, undefined, 2]));
console.log("arr_fn=" + JSON.stringify([1, fn, 2]));
console.log("arr_sym=" + JSON.stringify([1, sym, 2]));
console.log("arr_all_null=" + JSON.stringify([undefined, fn, sym]));

// --- array esparso: holes tambem viram null
const sparse: any[] = [1];
sparse[3] = 4;
console.log("arr_hole=" + JSON.stringify(sparse));

// --- aninhado
console.log("nested=" + JSON.stringify({ a: [undefined, { b: undefined, c: 3 }] }));
console.log("mix=" + JSON.stringify({ list: [fn], gone: fn }));

// --- toJSON retornando undefined
const t: any = { toJSON: function () { return undefined; } };
console.log("tojson_undef_top_isundef=" + (JSON.stringify(t) === undefined));
console.log("tojson_undef_in_obj=" + JSON.stringify({ a: t, b: 1 }));
console.log("tojson_undef_in_arr=" + JSON.stringify([t, 1]));

// --- replacer retornando undefined deleta a chave
console.log("replacer_undef=" + JSON.stringify({ a: 1, b: 2 }, function (k: any, v: any) {
  return k === "b" ? undefined : v;
}));
