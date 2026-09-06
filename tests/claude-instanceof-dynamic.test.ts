import { describe, test, expect } from "rts:test";

// `v instanceof C` com o lado DIREITO por VALOR (issue #2072).
//
// O motor resolvia `instanceof` só quando o lado direito NOMEAVA uma classe que
// ele pudesse checar em tempo de compilação. Um identificador LOCAL caía nesse
// mesmo caminho como se a sua grafia fosse o nome da classe: `check(v, C)`
// procurava uma classe chamada literalmente "C", não achava, e respondia
// `false` — errado, silencioso e plausível. Pior que recusar.
//
// A correção mantém o caminho estático primeiro (é o rápido e já correto) e
// manda todo o resto para um instanceof de RUNTIME: o trampolim
// `__rtsadp_instanceof_dyn` recupera o NOME da classe a partir do valor (o
// registro que o reify popula) e delega à mesma caminhada de protótipo que o
// caminho por nome usa. Mesmo mecanismo, alcançado por dado de runtime em vez
// de um literal de compilação.
//
// `instanceof` também ganhou variante própria no HIR: emitir um instanceof de
// runtime nunca pode ser no que um operador não-modelado decai.
//
// A divergência que este ficheiro registava ACABOU, e esta é a mudança separada
// que o parágrafo anterior anunciava. Dizia: "com um lado direito não-construtor
// (`v instanceof 2`, `v instanceof null`) o Node lança TypeError e o RTS
// responde `false` … o TypeError é semântica de spec própria e vira mudança
// separada".
//
// `InstanceofOperator` recusa antes de decidir seja o que for: um lado direito
// que não é objeto, um `Symbol.hasInstance` presente e não-chamável, um lado
// direito não-chamável, e um `prototype` que não é objeto. As quatro respondiam
// `false`, e `false` é a única resposta que não se distingue de uma legítima —
// `x instanceof algumObjeto` é uma gralha, e lia-se como "não, x não é um
// desses" em todos os programas que a escreviam.

class Animal {}
class Cachorro extends Animal {}
class Outra {}

const bicho = new Animal();
const cao = new Cachorro();

function check(v: any, C: any): boolean {
  return v instanceof C;
}

// ── o caso quebrado: a classe chega como PARÂMETRO ──────────────────────────
const dinMesmaClasse = check(bicho, Animal);
const dinOutraClasse = check(bicho, Outra);
const dinIrma = check(bicho, Cachorro);

// ── herança pelo caminho dinâmico ───────────────────────────────────────────
const dinHeranca = check(cao, Animal);
const dinExata = check(cao, Cachorro);

// ── lado direito não-construtor: RECUSADO, como no Node ─────────────────────
function refusa(v: any, C: any): string {
  try {
    check(v, C);
    return "nao lancou";
  } catch (e: any) {
    return e.constructor.name;
  }
}
const dinNumero = refusa(bicho, 2);
const dinNulo = refusa(bicho, null);

// O lado ESQUERDO primitivo continua a responder `false` sem lançar, que é o
// que a linguagem diz: `OrdinaryHasInstance` responde `false` para um operando
// esquerdo que não é objeto, e só o direito é que recusa.
const dinPrimitivo = check(5, Animal);
const dinString = check("x", Animal);

// ── classe por VARIÁVEL e por PROPRIEDADE (antes bailava a compilação) ──────
const apelido = Animal;
const porVariavel = bicho instanceof apelido;

const registro = { Ctor: Cachorro };
const porPropriedade = cao instanceof registro.Ctor;
const porPropriedadeNao = bicho instanceof registro.Ctor;

// ── o caminho ESTÁTICO não pode regredir ────────────────────────────────────
const estMesmaClasse = bicho instanceof Animal;
const estOutraClasse = bicho instanceof Outra;
const estHeranca = cao instanceof Animal;
const estErro = new TypeError("x") instanceof Error;
const estObjeto = {} instanceof Object;

describe("instanceof com classe por valor (#2072)", () => {
  test("classe como parâmetro decide certo", () => {
    expect(dinMesmaClasse).toBe(true);
    expect(dinOutraClasse).toBe(false);
    expect(dinIrma).toBe(false);
  });

  test("herança atravessa o caminho dinâmico", () => {
    expect(dinHeranca).toBe(true);
    expect(dinExata).toBe(true);
  });

  test("lado direito não-construtor é recusado, como no Node", () => {
    expect(dinNumero).toBe("TypeError");
    expect(dinNulo).toBe("TypeError");
  });

  test("primitivo nunca é instância", () => {
    expect(dinPrimitivo).toBe(false);
    expect(dinString).toBe(false);
  });

  test("classe alcançada por variável e por propriedade", () => {
    expect(porVariavel).toBe(true);
    expect(porPropriedade).toBe(true);
    expect(porPropriedadeNao).toBe(false);
  });

  test("o caminho estático segue correto", () => {
    expect(estMesmaClasse).toBe(true);
    expect(estOutraClasse).toBe(false);
    expect(estHeranca).toBe(true);
    expect(estErro).toBe(true);
    expect(estObjeto).toBe(true);
  });
});
