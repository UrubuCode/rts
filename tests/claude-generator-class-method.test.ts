import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator como MEMBRO de classe. Antes, um membro `*m()` ia direto para o
// eager-buffer (que so' expressa `yield` em posicao de STATEMENT), entao um
// `const r = yield x` sobrevivia ao desugar e morria no lowering com
// "expression raw/unrecognized: Yield". Agora o membro elegivel e' levantado
// para uma decl top-level e passa pela state-machine lazy.

// ── 1. metodo simples ───────────────────────────────────────────────────────
class Simples {
  *itens() {
    yield "a";
    yield "b";
  }
}
const s = new Simples();
const itSimples = s.itens();
const s1 = itSimples.next().value;
const s2 = itSimples.next().value;

// ── 2. metodo lendo this.campo ──────────────────────────────────────────────
class ComCampo {
  base: i64 = 10;
  *somas() {
    yield this.base + 1;
    yield this.base + 2;
  }
}
const cc = new ComCampo();
let somaCampo = 0;
for (const v of cc.somas()) {
  somaCampo = somaCampo + v;
}

// ── 3. yield COMO VALOR: o valor de `next(v)` volta no yield ────────────────
// E' o caso que bailava. Exige suspensao real (state-machine), o eager nao
// tem como alimentar o valor de volta.
class Acumulador {
  *soma() {
    let total = 0;
    while (true) {
      const x = yield total;
      total = total + x;
    }
  }
}
const acc = new Acumulador().soma();
const a0 = acc.next().value;
const a1 = acc.next(5).value;
const a2 = acc.next(7).value;

// ── 4. break dentro do laco ─────────────────────────────────────────────────
class ComBreak {
  *ate(n: i64) {
    let i = 0;
    while (true) {
      if (i >= n) {
        break;
      }
      yield i;
      i = i + 1;
    }
  }
}
let comBreak = "";
for (const v of new ComBreak().ate(3)) {
  comBreak = comBreak + `${v}`;
}

// ── 5. yield* delegando de dentro do metodo ─────────────────────────────────
function* interno() {
  yield 2;
  yield 3;
}
class Delegante {
  *tudo() {
    yield 1;
    yield* interno();
    yield 4;
  }
}
let delegado = "";
for (const v of new Delegante().tudo()) {
  delegado = delegado + `${v}`;
}

// ── 6. metodo ESTATICO gerador ──────────────────────────────────────────────
class Estatica {
  static *faixa(n: i64) {
    let i = 0;
    while (i < n) {
      const passo = yield i;
      i = i + 1;
    }
  }
}
const est = Estatica.faixa(3);
const e0 = est.next().value;
const e1 = est.next().value;

// ── 7. metodo PRIVADO gerador, consumido por um metodo publico ──────────────
class ComPrivado {
  *#internos() {
    let i = 0;
    while (i < 3) {
      const p = yield i * 2;
      i = i + 1;
    }
  }
  primeiro() {
    return this.#internos().next().value;
  }
}
const priv = new ComPrivado().primeiro();

// ── 8. for-of DIRETO sobre a chamada do metodo ──────────────────────────────
// Sem binding intermediario: exercita o call site de for-of, nao o de `const`.
class Direto {
  *tres() {
    let i = 0;
    while (i < 3) {
      const q = yield i + 1;
      i = i + 1;
    }
  }
}
let forOfDireto = "";
for (const v of new Direto().tres()) {
  forOfDireto = forOfDireto + `${v}`;
}

// ── 9. `super` no metodo: INELEGIVEL para o hoist, segue o caminho eager ────
// Guarda de regressao — o membro nao pode ser levantado (nao ha binding de
// super no topo) e precisa continuar funcionando como antes.
class Pai {
  rotulo(): string {
    return "pai";
  }
}
class Filho extends Pai {
  *comSuper() {
    yield super.rotulo();
    yield "filho";
  }
}
let comSuper = "";
for (const v of new Filho().comSuper()) {
  comSuper = comSuper + v;
}

describe("claude: generator como membro de classe", () => {
  test("metodo simples devolve iterador com .next()", () => {
    expect(s1).toBe("a");
    expect(s2).toBe("b");
  });

  test("metodo le this.campo", () => {
    expect(somaCampo).toBe(23);
  });

  test("yield como valor recebe o argumento de next(v)", () => {
    expect(a0).toBe(0);
    expect(a1).toBe(5);
    expect(a2).toBe(12);
  });

  test("break corta o laco do metodo", () => {
    expect(comBreak).toBe("012");
  });

  test("yield* delega de dentro do metodo", () => {
    expect(delegado).toBe("1234");
  });

  test("metodo estatico gerador", () => {
    expect(e0).toBe(0);
    expect(e1).toBe(1);
  });

  test("metodo privado gerador", () => {
    expect(priv).toBe(0);
  });

  test("for-of direto sobre a chamada do metodo", () => {
    expect(forOfDireto).toBe("123");
  });

  test("metodo com super continua no caminho eager", () => {
    expect(comSuper).toBe("paifilho");
  });
});
