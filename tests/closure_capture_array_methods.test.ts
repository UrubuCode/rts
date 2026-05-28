import { describe, test, expect } from "rts:test";

// (#195) Captura de variavel POR VALOR em arrow de array method (map/filter/
// forEach). Antes, capturar um param/local de fn numa arrow fazia o lift
// abortar e o promote-to-global produzir um simbolo divergente (valor nunca
// chegava) ou SIGILL. Agora a fn liftada recebe as capturas como params
// iniciais e o call site reifica via REIFY_CAPTURED (bound_args), com
// semantica POR-ATIVACAO (cada chamada da fn enclosing tem suas proprias
// capturas — curry/recursao corretos).

// captura escalar (param) em map
function addK(arr: number[], k: number): string {
  return arr.map(x => x + k).join(",");
}

// captura por-ativacao: duas chamadas independentes nao se atropelam
function makeAdder(k: number): number[] {
  return [1, 2, 3].map(x => x + k);
}

// captura de threshold em filter
function over(arr: number[], min: number): string {
  return arr.filter(x => x > min).join(",");
}

// captura de ARRAY + index dinamico (padrao zip)
function zipAdd(a: number[], b: number[]): string {
  return a.map((x, i) => x + b[i]).join(",");
}

// captura de fator em map
function scale(arr: number[], factor: number): string {
  return arr.map(x => x * factor).join(",");
}

const a = addK([1, 2, 3], 10);
const b1 = makeAdder(10).join(",");
const b2 = makeAdder(100).join(",");
const c = over([1, 5, 2, 8, 3], 3);
const d = zipAdd([1, 2, 3], [10, 20, 30]);
const e = scale([1, 2, 3], 5);

describe("closure capture in array methods (#195)", () => {
  test("scalar capture in map", () => expect(a).toBe("11,12,13"));
  test("per-activation capture A", () => expect(b1).toBe("11,12,13"));
  test("per-activation capture B (independent)", () => expect(b2).toBe("101,102,103"));
  test("threshold capture in filter", () => expect(c).toBe("5,8"));
  test("array capture with dynamic index", () => expect(d).toBe("11,22,33"));
  test("factor capture in map", () => expect(e).toBe("5,10,15"));
});
