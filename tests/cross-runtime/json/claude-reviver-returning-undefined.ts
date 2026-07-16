// Cross-runtime: reviver retornando undefined DELETA a chave.
// --- deleta uma chave de objeto
const o1: any = JSON.parse('{"a":1,"b":2,"c":3}', function (k: any, v: any) {
  return k === "b" ? undefined : v;
});
console.log("obj_deleted=" + JSON.stringify(o1));
console.log("obj_has_b=" + ("b" in o1));
console.log("obj_keys=" + Object.keys(o1).join(","));

// --- deleta TODAS as chaves
const o2: any = JSON.parse('{"a":1,"b":2}', function (k: any, v: any) {
  return k === "" ? v : undefined;
});
console.log("obj_all_deleted=" + JSON.stringify(o2));

// --- em array: deletar deixa um HOLE (length preservado), nao remove o slot
const a1: any = JSON.parse("[1,2,3]", function (k: any, v: any) {
  return v === 2 ? undefined : v;
});
console.log("arr_json=" + JSON.stringify(a1));
console.log("arr_len=" + a1.length);
console.log("arr_has_1=" + (1 in a1));
console.log("arr_idx1=" + String(a1[1]));
console.log("arr_keys=" + Object.keys(a1).join(","));

// --- deletar todos os elementos do array
const a2: any = JSON.parse("[1,2]", function (k: any, v: any) {
  return k === "" ? v : undefined;
});
console.log("arr_all_json=" + JSON.stringify(a2));
console.log("arr_all_len=" + a2.length);

// --- reviver na RAIZ retornando undefined => resultado e undefined
const root = JSON.parse('{"a":1}', function (k: any, v: any) {
  return k === "" ? undefined : v;
});
console.log("root_isundef=" + (root === undefined));
console.log("root_str=" + String(root));

// --- undefined so vale se RETORNADO; nao-retorno explicito idem (fn sem return)
const o3: any = JSON.parse('{"a":1,"b":2}', function (k: any, v: any) {
  if (k === "a") return v;
  if (k === "") return v;
  // sem return => undefined => deleta
});
console.log("implicit_undef=" + JSON.stringify(o3));

// --- deletar chave aninhada
const nested: any = JSON.parse('{"outer":{"keep":1,"drop":2}}', function (k: any, v: any) {
  return k === "drop" ? undefined : v;
});
console.log("nested=" + JSON.stringify(nested));

// --- null NAO deleta (so undefined deleta)
const withNull: any = JSON.parse('{"a":1,"b":2}', function (k: any, v: any) {
  return k === "b" ? null : v;
});
console.log("null_kept=" + JSON.stringify(withNull));
console.log("null_has_b=" + ("b" in withNull));

// --- 0 / "" / false tambem NAO deletam
const falsy: any = JSON.parse('{"a":1,"b":2,"c":3}', function (k: any, v: any) {
  if (k === "a") return 0;
  if (k === "b") return "";
  if (k === "c") return false;
  return v;
});
console.log("falsy_kept=" + JSON.stringify(falsy));
