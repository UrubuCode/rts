import { describe, test, expect } from "rts:test";

// (bundle real) `yield` em posição de SUBEXPRESSÃO.
//
// A state-machine modelava `yield` em duas posições: statement isolado
// (`yield E;`) e valor de uma ligação simples (`const a = yield E`). Qualquer
// outra posição fazia o build cair no eager-buffer, que só sabe expressar
// `yield` como `push` — e o yield de valor chegava cru ao lowering:
//
//   function* g(t, n) { return t && (yield n); }
//   // → "expression raw/unrecognized: Yield(...)"
//
// Era a causa MEDIDA do cluster restante de uma carga real (7 ocorrências,
// presentes em todos os bundles grandes) — `return a && (yield b)` é saída
// padrão do Babel. Seis formas caíam aqui: `return yield x`, `&&`, `||`,
// ternário, argumento de call e operando aritmético.
//
// A correção NÃO fatia expressão em estados: normaliza o corpo ANTES do
// SmBuilder para as duas formas que a máquina já modela
// (`generator_sm_normalize`). Nenhum caso novo na máquina, nenhum gate
// afrouxado.
//
// Os dois jeitos de errar isto, ambos cobertos abaixo:
//   1. CURTO-CIRCUITO — em `t && (yield n)` com `t` falsy o yield NÃO executa e
//      o generator NÃO suspende. Avaliar o yield antes do teste mudaria o
//      comportamento observável.
//   2. ORDEM DE AVALIAÇÃO — em `f() + (yield a)`, `f()` roda ANTES da
//      suspensão. Içar só o yield faria `f()` rodar depois.
//
// Todos os valores conferidos contra o Node. Pré-computado no top-level.

// ── 1. `&&` — falsy não suspende ───────────────────────────────────────────
function* andG(t: any, n: any) { return t && (yield n); }
const andFalsy: any = andG(0, "N");
const andFalsySeq = JSON.stringify([andFalsy.next(), andFalsy.next()]);
const andTruthy: any = andG(1, "N");
const andTruthySeq = JSON.stringify([andTruthy.next(), andTruthy.next("V")]);

// ── 2. `||` ────────────────────────────────────────────────────────────────
function* orG(t: any) { const a = t || (yield "ask"); return a; }
const orHave: any = orG("have");
const orHaveSeq = JSON.stringify([orHave.next(), orHave.next()]);
const orAsk: any = orG("");
const orAskSeq = JSON.stringify([orAsk.next(), orAsk.next("got")]);

// ── 3. `return yield x` (a menor forma que falhava) ────────────────────────
function* retY(t: any) { return yield t; }
const ret1: any = retY(7);
const retSeq = JSON.stringify([ret1.next(), ret1.next("R")]);

// ── 4. ternário — o yield fica DENTRO do ramo ──────────────────────────────
function* condG(t: any) { const a = t ? (yield "yes") : "no"; return a; }
const condYes: any = condG(1);
const condYesSeq = JSON.stringify([condYes.next(), condYes.next("Y")]);
const condNo: any = condG(0);
const condNoSeq = JSON.stringify([condNo.next(), condNo.next()]);

// ── 5. ordem de avaliação em torno da suspensão ────────────────────────────
const order: any[] = [];
function tag(x: any) { order.push("tag:" + x); return "t(" + x + ")"; }
function* callG() { const a = tag(1) + (yield "mid") + tag(2); return a; }
const call1: any = callG();
const callFirst = JSON.stringify(call1.next());
const callSecond = JSON.stringify(call1.next("M"));
const callOrder = order.join(",");

// ── 6. operando aritmético ─────────────────────────────────────────────────
function* addG(t: any) { const a = 1 + (yield t); return a; }
const add1: any = addG(10);
const addSeq = JSON.stringify([add1.next(), add1.next(5)]);

// ── 7. `this` do receptor sobrevive ao temporário ──────────────────────────
// `o.mul(yield 3)` NÃO pode virar `let t = o.mul; t(...)` — perderia o `this`.
function* methG(o: any) { const a = o.mul(yield 3); return a; }
const recv: any = { k: 10, mul(x: any) { return this.k * x; } };
const meth1: any = methG(recv);
const methSeq = JSON.stringify([meth1.next(), meth1.next(4)]);

// ── 8. `??` — só null/undefined vai para a direita ─────────────────────────
function* nulG(t: any) { const a = t ?? (yield "fill"); return a; }
const nulZero: any = nulG(0);
const nulZeroSeq = JSON.stringify([nulZero.next(), nulZero.next()]);
const nulNull: any = nulG(null);
const nulNullSeq = JSON.stringify([nulNull.next(), nulNull.next("F")]);

// ── 9. dois yields no MESMO statement ──────────────────────────────────────
function* twoG() { const a = (yield "p") + "-" + (yield "q"); return a; }
const two1: any = twoG();
const twoSeq = JSON.stringify([two1.next(), two1.next("A"), two1.next("B")]);

// ── 10. combinado com for-of (a fatia anterior) ────────────────────────────
function* loopG(src: any) {
  for (const x of src) {
    const a = x && (yield x);
    if (a) yield "!" + a;
  }
}
const loop1: any = loopG([0, 1, 2]);
const loopSeq = JSON.stringify([
  loop1.next(), loop1.next(), loop1.next("Z"), loop1.next(), loop1.next(),
]);

// ── 11. casos de guarda: onde uma normalizacao PLAUSIVEL erra em silencio ──
// Cada um destes passa num teste que olhe so' o valor final; o que os separa e'
// a SEQUENCIA de `next(v)` e a ordem dos efeitos colaterais.

// Efeito colateral a ESQUERDA: `f()` roda ANTES da suspensao. Icar so' o yield
// faria `f()` rodar DEPOIS — invisivel sem efeito observavel.
let ordem = "";
function fx(): any { ordem = ordem + "f"; return 1; }
function* sideG() { const r = fx() + (yield 2); return ordem + ":" + r; }
const side1: any = sideG();
const sideSeq = JSON.stringify([side1.next(), side1.next(10)]);

// Curto-circuito com LITERAL: o generator termina sem NUNCA pedir valor.
function* litAnd() { const a = 0 && (yield 1); return a; }
function* litOr() { const a = 1 || (yield 1); return a; }
const la: any = litAnd();
const litAndSeq = JSON.stringify([la.next(), la.next()]);
const lo: any = litOr();
const litOrSeq = JSON.stringify([lo.next(), lo.next()]);

// yield dos DOIS lados do operador.
function* bothG() { return (yield 1) + (yield 2); }
const both1: any = bothG();
const bothSeq = JSON.stringify([both1.next(), both1.next(10), both1.next(20)]);

// yield num elemento de array COM elemento depois (a ordem tem de sobreviver).
function* arrG() { return [(yield 1), 9]; }
const arr1: any = arrG();
const arrSeq = JSON.stringify([arr1.next(), arr1.next(7)]);

// yield no TESTE de um `if` — icar para fora do `if` e' correto (o teste roda
// uma vez), ao contrario do teste de um laco, que a normalizacao recusa.
function* ifG(t: any) { if (yield t) { return "sim"; } return "nao"; }
const ifTrue: any = ifG("q");
const ifTrueSeq = JSON.stringify([ifTrue.next(), ifTrue.next(true)]);
const ifFalse: any = ifG("q");
const ifFalseSeq = JSON.stringify([ifFalse.next(), ifFalse.next(false)]);

// DUPLA suspensao na mesma expressao, com `next(v)` distinto em cada, e tres
// efeitos colaterais que so' saem na ordem certa se cada trecho rodar no seu
// estado: A antes da 1a suspensao, B entre as duas, C depois da 2a.
let marca = "";
function mk(x: any) { marca = marca + x; return x; }
function* dblG() {
  const r = mk("A") + (yield "s1") + mk("B") + (yield "s2") + mk("C");
  return r;
}
const dbl: any = dblG();
const dblFirst = JSON.stringify(dbl.next());
const dblSecond = JSON.stringify(dbl.next("|1|"));
const dblThird = JSON.stringify(dbl.next("|2|"));
const dblOrder = marca;

// ── 12. controles: formas que JA' passavam, byte a byte iguais ─────────────
function* ctlBind(t: any) { const a = yield t; return a; }
function* ctlIf(x: any) { if (x) yield x; yield "fim"; }
function* ctlTernStmt(x: any) { yield x ? x : 0; }
function* ctlStar() { yield* [1, 2]; yield 3; }
class CtlK { *m(a: any) { const r = yield a; return r; } }
const cb: any = ctlBind(5);
const ctlBindSeq = JSON.stringify([cb.next(), cb.next("S")]);
const ctlIfSeq = JSON.stringify([...ctlIf(1)]);
const ctlTernSeq = JSON.stringify([...ctlTernStmt(4)]);
const ctlStarSeq = JSON.stringify([...ctlStar()]);
const ck: any = new CtlK().m(1);
const ctlClassSeq = JSON.stringify([ck.next(), ck.next("M")]);

describe("generator: yield em posicao de subexpressao", () => {
  test("&& com esquerdo falsy NAO suspende", () =>
    expect(andFalsySeq).toBe('[{"value":0,"done":true},{"done":true}]'));
  test("&& com esquerdo truthy suspende", () =>
    expect(andTruthySeq).toBe('[{"value":"N","done":false},{"value":"V","done":true}]'));
  test("|| com esquerdo truthy NAO suspende", () =>
    expect(orHaveSeq).toBe('[{"value":"have","done":true},{"done":true}]'));
  test("|| com esquerdo falsy suspende", () =>
    expect(orAskSeq).toBe('[{"value":"ask","done":false},{"value":"got","done":true}]'));
  test("return yield x", () =>
    expect(retSeq).toBe('[{"value":7,"done":false},{"value":"R","done":true}]'));
  test("ternario ramo com yield", () =>
    expect(condYesSeq).toBe('[{"value":"yes","done":false},{"value":"Y","done":true}]'));
  test("ternario ramo sem yield NAO suspende", () =>
    expect(condNoSeq).toBe('[{"value":"no","done":true},{"done":true}]'));
  test("suspende no meio da concatenacao", () =>
    expect(callFirst).toBe('{"value":"mid","done":false}'));
  test("retoma e completa a expressao", () =>
    expect(callSecond).toBe('{"value":"t(1)Mt(2)","done":true}'));
  test("ordem: esquerda antes de suspender, direita depois", () =>
    expect(callOrder).toBe("tag:1,tag:2"));
  test("operando aritmetico", () =>
    expect(addSeq).toBe('[{"value":10,"done":false},{"value":6,"done":true}]'));
  test("this do receptor preservado", () =>
    expect(methSeq).toBe('[{"value":3,"done":false},{"value":40,"done":true}]'));
  test("?? com 0 NAO vai para a direita", () =>
    expect(nulZeroSeq).toBe('[{"value":0,"done":true},{"done":true}]'));
  test("?? com null vai para a direita", () =>
    expect(nulNullSeq).toBe('[{"value":"fill","done":false},{"value":"F","done":true}]'));
  test("dois yields no mesmo statement", () =>
    expect(twoSeq).toBe(
      '[{"value":"p","done":false},{"value":"q","done":false},{"value":"A-B","done":true}]',
    ));
  test("combinado com for-of", () =>
    expect(loopSeq).toBe(
      '[{"value":1,"done":false},{"value":2,"done":false},{"value":"!Z","done":false},{"done":true},{"done":true}]',
    ));
});

describe("generator: yield em subexpressao — casos de guarda", () => {
  test("efeito colateral a esquerda roda ANTES da suspensao", () =>
    expect(sideSeq).toBe('[{"value":2,"done":false},{"value":"f:11","done":true}]'));
  test("`0 && (yield)` termina sem pedir valor", () =>
    expect(litAndSeq).toBe('[{"value":0,"done":true},{"done":true}]'));
  test("`1 || (yield)` termina sem pedir valor", () =>
    expect(litOrSeq).toBe('[{"value":1,"done":true},{"done":true}]'));
  test("yield dos dois lados do operador", () =>
    expect(bothSeq).toBe(
      '[{"value":1,"done":false},{"value":2,"done":false},{"value":30,"done":true}]',
    ));
  test("yield em elemento de array com elemento depois", () =>
    expect(arrSeq).toBe('[{"value":1,"done":false},{"value":[7,9],"done":true}]'));
  test("yield no teste de if — ramo verdadeiro", () =>
    expect(ifTrueSeq).toBe('[{"value":"q","done":false},{"value":"sim","done":true}]'));
  test("yield no teste de if — ramo falso", () =>
    expect(ifFalseSeq).toBe('[{"value":"q","done":false},{"value":"nao","done":true}]'));
  test("dupla suspensao: primeira parada", () =>
    expect(dblFirst).toBe('{"value":"s1","done":false}'));
  test("dupla suspensao: segunda parada", () =>
    expect(dblSecond).toBe('{"value":"s2","done":false}'));
  test("dupla suspensao: os dois next(v) entram na posicao certa", () =>
    expect(dblThird).toBe('{"value":"A|1|B|2|C","done":true}'));
  test("dupla suspensao: efeitos na ordem A,B,C", () => expect(dblOrder).toBe("ABC"));

  test("controle: const a = yield t", () =>
    expect(ctlBindSeq).toBe('[{"value":5,"done":false},{"value":"S","done":true}]'));
  test("controle: if (x) yield x", () => expect(ctlIfSeq).toBe('[1,"fim"]'));
  test("controle: yield x ? x : 0 em statement", () => expect(ctlTernSeq).toBe("[4]"));
  test("controle: yield*", () => expect(ctlStarSeq).toBe("[1,2,3]"));
  test("controle: metodo de classe", () =>
    expect(ctlClassSeq).toBe('[{"value":1,"done":false},{"value":"M","done":true}]'));
});
