// Cross-runtime: JSON.parse de estrutura aninhada profunda.
// Profundidades modestas e deterministicas (sem estourar stack de propósito).

// --- arrays aninhados: [[[...1...]]]
function nestArray(depth: number): string {
  let s = "1";
  for (let i = 0; i < depth; i++) s = "[" + s + "]";
  return s;
}
function unwrapArray(v: any): number {
  let d = 0;
  while (Array.isArray(v)) { v = v[0]; d++; }
  return d;
}
const a10: any = JSON.parse(nestArray(10));
console.log("arr10_depth=" + unwrapArray(a10));
console.log("arr10_leaf=" + a10[0][0][0][0][0][0][0][0][0][0]);

const a50: any = JSON.parse(nestArray(50));
console.log("arr50_depth=" + unwrapArray(a50));

const a200: any = JSON.parse(nestArray(200));
console.log("arr200_depth=" + unwrapArray(a200));

// --- objetos aninhados: {"a":{"a":{...}}}
function nestObject(depth: number): string {
  let s = "1";
  for (let i = 0; i < depth; i++) s = '{"a":' + s + "}";
  return s;
}
function unwrapObject(v: any): number {
  let d = 0;
  while (v !== null && typeof v === "object") { v = v.a; d++; }
  return d;
}
const o10: any = JSON.parse(nestObject(10));
console.log("obj10_depth=" + unwrapObject(o10));
console.log("obj10_leaf=" + o10.a.a.a.a.a.a.a.a.a.a);

const o100: any = JSON.parse(nestObject(100));
console.log("obj100_depth=" + unwrapObject(o100));

// --- alternando objeto/array
let alt = "7";
for (let i = 0; i < 20; i++) alt = i % 2 === 0 ? "[" + alt + "]" : '{"k":' + alt + "}";
const altParsed: any = JSON.parse(alt);
console.log("alt_leaf=" + altParsed.k[0].k[0].k[0].k[0].k[0].k[0].k[0].k[0].k[0].k[0]);

// --- round-trip preserva a profundidade
const deep: any = JSON.parse(nestArray(30));
console.log("roundtrip=" + (JSON.stringify(deep) === nestArray(30)));
console.log("roundtrip_obj=" + (JSON.stringify(JSON.parse(nestObject(30))) === nestObject(30)));

// --- estrutura larga E profunda
const wide: any = JSON.parse('{"a":[{"b":[1,2,{"c":{"d":[3,{"e":4}]}}]}]}');
console.log("wide_e=" + wide.a[0].b[2].c.d[1].e);
console.log("wide_d0=" + wide.a[0].b[2].c.d[0]);
console.log("wide_json=" + JSON.stringify(wide));

// --- reviver visita todos os niveis (conta chamadas em profundidade fixa)
let calls = 0;
JSON.parse(nestArray(10), function (k: any, v: any) { calls++; return v; });
console.log("reviver_calls=" + calls);

// --- reviver transforma folha em profundidade
const doubled: any = JSON.parse(nestArray(5), function (k: any, v: any) {
  return typeof v === "number" ? v * 2 : v;
});
console.log("reviver_leaf=" + doubled[0][0][0][0][0]);

// --- stringify de estrutura profunda com espacos conta linhas
const spaced = JSON.stringify(JSON.parse(nestObject(5)), null, 1);
console.log("spaced_lines=" + spaced.split("\n").length);
