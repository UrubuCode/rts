import { describe, test, expect } from "rts:test";

// Rodada 2 dos gaps de bundle real (WhatsApp Web) — 9 falhas de carga, 5 delas
// o MESMO padrão: `asyncToGenerator(function*(){ try { x = yield f() } catch(e)
// { throw g(), e } })`, o async/await transpilado do Babel. Fixes cobertos:
//
// 1. generator SM ganhou `Stmt::Throw` (verbatim + done fora de região
//    protegida; recusa honesta dentro de try/finally modelado). Sem o arm, um
//    único `throw` — presente em todo catch do Babel — derrubava o generator
//    inteiro para o eager-buffer, que não expressa yield de valor.
// 2. corpo com `throw` agora conta como "precisa de lazy": o eager executava o
//    corpo INTEIRO na construção e o throw disparava em `g()`, fora do try real.
// 3. `.next()/.return()/.throw()` de generator emitem o post-call error check —
//    sem ele `try { it.next() } catch` perdia o throw (aflorava fora do try).
// 4. bitwise compound (`<<=` etc.) em cell local e gcell (antes: bail).
// 5. lifter: fn-irmã declarada dentro de try/switch/for agora é visível
//    (collect_arrow_decls/collect_declared exaustivos); arrow sob rótulo e sob
//    cast é levantada (arms Labeled/Cast no rewrite).
// 6. `C.m()` onde `m` é static NÃO declarado no corpo (`C.m = fn` depois) chama
//    o valor via o caminho dinâmico em vez de bailar; e um nome capturado/gcell
//    não é mais roubado pelo caminho estático de classe homônima.
//
// Valores conferidos contra o Node (generators em módulo; item 5-sibling em
// script sloppy, onde function-em-bloco iça à la Annex B como nas páginas).

function d(x: any) { return x + 1; }
function fmk(n: any) { return { success() { return "ok:" + n; } }; }
function ymk(e: any) { return "Y:" + e; }

function drive(g: any, args: any, sends: any): string {
  const it = g.apply(null, args);
  let out = "", res = it.next();
  let k = 0;
  while (!res.done) { out += "[y " + res.value + "]"; res = it.next(sends[k]); k = k + 1; }
  return out + "[ret " + res.value + "]";
}

// ── padrão Babel destilado: yield-valor em try + throw no catch ─────────────
const g1 = function* (t: any) {
  try {
    var l = yield t();
    return "done:" + l, l;
  } catch (e) {
    throw "logged", e;
  }
};
const r1 = drive(g1, [() => 7], [42]);

// ── cond && (r = yield f()) + multi-declarator com yield + catch-rethrow ────
const g2 = function* (e: any, t: any) {
  var n = e.map(d), r;
  t === 1 && (r = yield fmk(n.length));
  try {
    var a, i = yield ymk(e);
    return (a = r) == null || a.success(), i;
  } catch (e2) {
    throw e2;
  }
};
const r2 = drive(g2, [[1, 2], 1], [fmk(9), "II"]);

// ── throw de topo (sem try nenhum) não pode derrubar a compilação ───────────
const g3 = function* (e: any, t: any) {
  var n = "req:" + e, r = yield n, a = "parsed:" + r;
  if (a) return a;
  throw new Error("fail");
};
const r3 = drive(g3, ["E", 0], ["RESP"]);

// ── throw não capturado: propaga ao chamador de .next() e ENCERRA ───────────
function* gt(): any { yield 1; throw new Error("boom"); }
const it = gt();
let tprop = "y" + it.next().value;
try {
  it.next();
  tprop += "|NOTHROW";
} catch (e: any) {
  tprop += "|caught:" + e.message;
}
const after = it.next();
tprop += "|done:" + after.done + ":" + after.value;

// ── construção NÃO executa o corpo (o throw só dispara na retomada) ─────────
function* glazy(): any { yield 1; throw new Error("early"); }
let lazyOk = "ctor-ok";
const itl = glazy();          // Node: nada lança aqui
lazyOk += "|first:" + itl.next().value;

// ── bitwise compound em CELL (capturada e mutada por closure) ───────────────
function hashish(arr: any): any {
  let x = 1;
  arr.forEach(function (c: any) {
    x <<= 2;
    x |= c & 1;
    x ^= 3;
    x >>= 1;
  });
  return x;
}

// ── fn-irmã declarada dentro de try/switch, referência ADIANTE ──────────────
function mk(): any {
  var api = function () { return w() + z(); };
  try {
    function w(): any { return 1; }
  } catch (e) {}
  switch (1) {
    case 1:
      function z(): any { return 2; }
  }
  return api;
}

// ── arrow sob rótulo ────────────────────────────────────────────────────────
function lab(): any {
  let acc = 0;
  L: for (let i = 0; i < 3; i++) {
    const f = () => i;
    if (i === 1) continue L;
    acc += f();
  }
  return acc;
}

// ── fn DECL aninhada no corpo do generator (hoisted, usada antes) ───────────
const gA = function* (e: any) {
  var t = n();
  return yield ("save:" + t), e.length;
  function n(): any { return e.join("-"); }
};
const rA = drive(gA, [["a", "b"]], ["ok"]);

// ── bloco ROTULADO com yield + break rótulo (early-exit do Babel) ───────────
const gB = function* (t: any) {
  e: {
    if (t === "u") { yield "upd"; break e; }
    if (t === "r") { yield "rw1"; yield "rw2"; break e; }
    yield "other";
  }
  return "end";
};
const rB = drive(gB, ["r"], [0, 0, 0]);

// ── yield no TESTE de if (dentro de &&) ─────────────────────────────────────
function hy(): any { return true; }
const gC = function* () {
  if (hy() && (yield "ask") === true) return "yes";
  return "no";
};
const rC = drive(gC, [], [true]);

// ── static não declarado, atribuído depois, e CHAMADO ───────────────────────
class WaSet {}
(WaSet as any).add = function (v: any) { return "got:" + v; };
const dynstatic = (WaSet as any).add(5);

describe("gaps de bundle real — rodada 2 (throw em generator + lifter + cell)", () => {
  test("padrão Babel: yield-valor em try, throw no catch", () => {
    expect(r1).toBe("[y 7][ret 42]");
  });
  test("&& com atribuição-yield + multi-declarator", () => {
    expect(r2).toBe("[y [object Object]][y Y:1,2][ret II]");
  });
  test("throw de topo no fim do generator", () => {
    expect(r3).toBe("[y req:E][ret parsed:RESP]");
  });
  test("throw não capturado propaga e encerra o generator", () => {
    expect(tprop).toBe("y1|caught:boom|done:true:undefined");
  });
  test("construir o generator não roda o corpo", () => {
    expect(lazyOk).toBe("ctor-ok|first:1");
  });
  test("bitwise compound em cell local", () => {
    expect(hashish([5, 2])).toBe(7);
  });
  test("fn-irmã em try/switch vista pela referência adiante", () => {
    expect(mk()()).toBe(3);
  });
  test("arrow sob rótulo é levantada", () => {
    expect(lab()).toBe(2);
  });
  test("static dinâmico chamado", () => {
    expect(dynstatic).toBe("got:5");
  });
  test("fn decl aninhada no generator (hoisted, usada antes)", () => {
    expect(rA).toBe("[y save:a-b][ret 2]");
  });
  test("bloco rotulado com yield + break rótulo", () => {
    expect(rB).toBe("[y rw1][y rw2][ret end]");
  });
  test("yield no teste de if via &&", () => {
    expect(rC).toBe("[y ask][ret yes]");
  });
});
