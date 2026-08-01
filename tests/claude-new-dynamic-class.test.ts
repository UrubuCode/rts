import { describe, test, expect } from "rts:test";

// `new e(..)` sobre uma classe que NÃO é um nome de classe deste programa — um
// VALOR de classe alcançado por CAPTURA, por célula de global de módulo, ou por
// parâmetro de uma função aninhada.
//
// É a forma dominante em código minificado/transpilado, onde toda classe acaba
// atrás de um identificador de uma letra. Antes isso era um bail de COMPILAÇÃO:
// "`new e(..)` — class `e` is not a user class in this program".
//
// Duas coisas faltavam, e as duas estão cobertas aqui:
//   1. o roteamento: `new <valor>(..)` só era emitido quando o nome era um LOCAL
//      da função corrente; qualquer outra ligação caía no bail;
//   2. a análise de variável livre: o nome do CONSTRUTOR em `new e(..)` não
//      contava como referência a `e`, então a arrow não capturava nada e `e`
//      ficava não-ligado em tempo de execução.
//
// Valores conferidos contra o Node. Pré-computado no top-level (ler propriedade
// de instância dentro do closure de `test()` esbarra num bug pré-existente de
// leitura de campo em arrow).

class Ponto {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
  soma(): number {
    return this.x + this.y;
  }
}

class Cinco {
  s: number;
  constructor(a: number, b: number, c: number, d: number, e: number) {
    this.s = a + b + c + d + e;
  }
}

// ── capturada por uma arrow ─────────────────────────────────────────────────
const e = Ponto;
const viaArrow = () => {
  const p = new e(1, 2);
  return p.x;
};
const viaArrowValor = viaArrow();

const viaArrowMetodo = () => {
  const p = new e(3, 4);
  return p.soma();
};
const viaArrowMetodoValor = viaArrowMetodo();

// ── célula de global de módulo, lida de dentro de uma `function` ────────────
var g: any;
g = Ponto;
function viaGlobal(): number {
  const p = new g(5, 6);
  return p.x;
}
const viaGlobalValor = viaGlobal();

// ── parâmetro de uma arrow externa, usado por uma arrow interna ─────────────
const fabrica = (C: any) => () => {
  const p = new C(7, 8);
  return p.y;
};
const viaFabricaValor = fabrica(Ponto)();

// ── método de objeto literal capturando a ligação externa ───────────────────
const obj = {
  faz(): number {
    const p = new e(9, 10);
    return p.soma();
  },
};
const viaObjValor = obj.faz();

// ── mais de 4 argumentos (transbordo para o array de resto) ─────────────────
const c5 = Cinco;
function viaCinco(): number {
  const o = new c5(1, 2, 3, 4, 5);
  return o.s;
}
const viaCincoValor = viaCinco();

// ── formas ESTÁTICAS que não podem regredir ─────────────────────────────────
const direto = new Ponto(1, 2);
const diretoX = direto.x;
const diretoSoma = direto.soma();

// alias estático `const C = Ponto` (caminho estático, não o dinâmico)
const Alias = Ponto;
const viaAlias = new Alias(10, 20);
const viaAliasSoma = viaAlias.soma();

// classe passada como argumento e construída dentro de uma `function`
function constroi(C: any): number {
  const p = new C(2, 3);
  return p.soma();
}
const viaParametroValor = constroi(Ponto);

// `new` dentro de uma arrow sobre um nome de CLASSE de verdade não pode virar
// captura espúria (a análise de livres agora vê o nome; a lista de classes o
// filtra).
const classeDireta = () => {
  const p = new Ponto(4, 5);
  return p.soma();
};
const classeDiretaValor = classeDireta();

describe("new sobre classe capturada", () => {
  test("arrow captura a ligação e constrói", () => {
    expect(viaArrowValor).toBe(1);
  });

  test("método da instância construída na arrow", () => {
    expect(viaArrowMetodoValor).toBe(7);
  });

  test("célula de global de módulo lida de uma function", () => {
    expect(viaGlobalValor).toBe(5);
  });

  test("parâmetro da arrow externa visto pela interna", () => {
    expect(viaFabricaValor).toBe(8);
  });

  test("método de objeto literal capturando a ligação", () => {
    expect(viaObjValor).toBe(19);
  });

  test("mais de 4 argumentos transbordam corretamente", () => {
    expect(viaCincoValor).toBe(15);
  });
});

describe("caminhos estáticos não regridem", () => {
  test("new sobre o nome da classe", () => {
    expect(diretoX).toBe(1);
    expect(diretoSoma).toBe(3);
  });

  test("alias estático const C = Classe", () => {
    expect(viaAliasSoma).toBe(30);
  });

  test("classe por parâmetro de function", () => {
    expect(viaParametroValor).toBe(5);
  });

  test("nome de classe de verdade dentro de arrow", () => {
    expect(classeDiretaValor).toBe(9);
  });
});
