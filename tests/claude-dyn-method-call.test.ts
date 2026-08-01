import { describe, test, expect } from "rts:test";

// Campo-como-função e método DINÂMICO numa classe.
//
// `recv.m()` onde `m` não é um método sintetizado da classe (não existe
// `__rtsn_method_C_m`) recusava em tempo de COMPILAÇÃO — "no such method on
// class C (a field-as-function / dynamic method is a later increment)". O gate
// só aceitava um nome coletado em `desc.fields`, e essa coleta varre apenas o
// corpo do CONSTRUTOR: campo atribuído em outro método, escrita externa na
// instância, `Object.assign(this, …)` e método publicado no protótipo ficavam
// todos de fora.
//
// Em runtime as quatro formas são a MESMA pergunta — "o que o slot `m` deste
// receptor guarda?" — então resolvem por um caminho genérico só: lê o slot
// (próprio, depois a cadeia de protótipos) e invoca com `this` = receptor.
// Um slot sem função vira TypeError em runtime, que é o que o JS faz para
// `obj.nope()`; recusar em compile time é que divergia.
//
// Segunda correção, independente: o caminho genérico tinha ABI fixa de 3 slots
// e recusava chamada com mais de 3 args ou com `...spread` — inclusive para um
// campo DECLARADO, que já passava com 3. Agora os args vão num vec CONTADO, e
// `INVOKE_AUTO` vê a contagem real (bound args, param kinds, env de
// uniform-thunk, empacotamento do tail variádico).
//
// Valores conferidos contra o Node. Pré-computado no top-level (regra do
// projeto: método dentro de test() pode perder handle pro GC).

// ── o que JÁ passava (trava contra regressão) ───────────────────────────────

class CampoArrow {
  f = () => 1;
  g() { return this.f(); }
}
const campoArrowInterno = new CampoArrow().g();
const campoArrowExterno = new CampoArrow().f();
const instancia = new CampoArrow();
const viaVariavel = instancia.f;
const campoArrowViaVariavel = viaVariavel();

class AtribuiNoCtor {
  constructor() { this.f = () => 2; }
  g() { return this.f(); }
}
const atribuiNoCtor = new AtribuiNoCtor().g();

class RecebeDeFora {
  constructor(o) { this.f = o.f; }
  g() { return this.f(); }
}
const recebeDeFora = new RecebeDeFora({ f: () => 3 }).g();

const literal = { f: () => 4 };
const campoDeObjetoLiteral = literal.f();

const ClasseExpressao = class {
  constructor() { this.f = () => 5; }
  g() { return this.f(); }
};
const classeExpressao = new ClasseExpressao().g();

class AtribuiSobCondicional {
  constructor() {
    if (true) { this.f = () => 6; }
  }
  g() { return this.f(); }
}
const atribuiSobCondicional = new AtribuiSobCondicional().g();

class ArgCallback {
  f = (cb) => cb(7);
  g() { return this.f((x) => x); }
}
const argCallback = new ArgCallback().g();

// ── o que FALHAVA: slot não coletado como campo ─────────────────────────────

// Atribuído num método que NÃO é o construtor.
class AtribuiEmMetodo {
  init() { this.f = () => 11; }
  g() { return this.f(); }
}
const emMetodo = new AtribuiEmMetodo();
emMetodo.init();
const atribuiEmMetodo = emMetodo.g();

// Atribuído de FORA, na instância.
class SlotExterno {}
const externo = new SlotExterno();
externo.f = () => 12;
const atribuiDeFora = externo.f();

// `Object.assign(this, …)` no construtor — o alvo é `this`, mas não há
// `this.f =` sintático para a varredura enxergar.
const ViaObjectAssign = class {
  constructor() { Object.assign(this, { f: () => 13 }); }
  g() { return this.f(); }
};
const viaObjectAssign = new ViaObjectAssign().g();

// ── o que FALHAVA: campo declarado, mas a chamada excedia a ABI de 3 slots ──

class QuatroArgs {
  f = (a, b, c, d) => a + b + c + d;
  g() { return this.f(1, 2, 3, 4); }
}
const quatroArgs = new QuatroArgs().g();

class CincoArgs {
  f = (a, b, c, d, e) => a + b + c + d + e;
}
const cincoArgsDeFora = new CincoArgs().f(1, 2, 3, 4, 5);

class ComSpread {
  f = (...xs) => xs.length;
  g(...xs) { return this.f(...xs); }
}
const comSpread = new ComSpread().g(1, 2, 3);

class SpreadMisturado {
  f = (a, b, c, d) => a * 1000 + b * 100 + c * 10 + d;
  g() {
    const tail = [3, 4];
    return this.f(1, 2, ...tail);
  }
}
const spreadMisturado = new SpreadMisturado().g();

// NOTA — `C.prototype.m = fn` sobre uma `class` NÃO está coberto aqui.
// Continua falhando com `TypeError: m is not a function`, e é trabalho próprio:
// a escrita direta `A.prototype.h = fn` não pousa no objeto-protótipo
// compartilhado que `new A()` liga, enquanto a forma via variável
// (`const p = A.prototype; p.h = fn`) pousa. Um ctor-FUNÇÃO (`function A(){}`)
// já funciona pelas duas formas. Não incluo teste do caso quebrado neste
// arquivo para ele não mascarar os que estão de fato corrigidos.

// `this` NÃO-léxico: um campo `function` (não-arrow) enxerga o receptor.
class ThisDeCampoFuncao {
  constructor() {
    this.n = 42;
    this.f = function () { return this.n; };
  }
  g() { return this.f(); }
}
const thisDeCampoFuncao = new ThisDeCampoFuncao().g();

describe("campo-como-função / método dinâmico em classe", () => {
  test("campo-arrow declarado, chamado de dentro e de fora", () => {
    expect(campoArrowInterno).toBe(1);
    expect(campoArrowExterno).toBe(1);
    expect(campoArrowViaVariavel).toBe(1);
  });

  test("campo atribuído no construtor", () => {
    expect(atribuiNoCtor).toBe(2);
    expect(recebeDeFora).toBe(3);
    expect(classeExpressao).toBe(5);
    expect(atribuiSobCondicional).toBe(6);
  });

  test("campo-função em objeto literal", () => {
    expect(campoDeObjetoLiteral).toBe(4);
  });

  test("campo-função com argumento callback", () => {
    expect(argCallback).toBe(7);
  });

  test("slot atribuído fora do construtor", () => {
    expect(atribuiEmMetodo).toBe(11);
    expect(atribuiDeFora).toBe(12);
    expect(viaObjectAssign).toBe(13);
  });

  test("chamada além dos 3 slots da ABI fixa", () => {
    expect(quatroArgs).toBe(10);
    expect(cincoArgsDeFora).toBe(15);
  });

  test("chamada com spread", () => {
    expect(comSpread).toBe(3);
    expect(spreadMisturado).toBe(1234);
  });

  test("campo `function` enxerga o receptor como `this`", () => {
    expect(thisDeCampoFuncao).toBe(42);
  });
});
