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

// (#195 followup) captura de array + `.length` da captura + indice modular
// (padrao rolling key / 353_obf_xor_decode). `.length` da captura despacha
// via UNIVERSAL_LENGTH (a captura eh marcada ambigua no lifted fn).
function rolling(arr: number[], keys: number[]): string {
  return arr.map((c, i) => keys[i % keys.length]).join(",");
}

// (#195 followup) reduce com captura: callback (acc, val) capturando `base`/
// `sep`/`mul` do escopo. Com init e sem init.
function sumPlus(arr: number[], base: number): number {
  return arr.reduce((acc, x) => acc + x + base, 0);
}
function joinSep(arr: string[], sep: string): string {
  return arr.reduce((acc, x) => acc + sep + x, "");
}
function reduceMul(arr: number[], mul: number): number {
  return arr.reduce((acc, x) => acc + x * mul);
}

const a = addK([1, 2, 3], 10);
const b1 = makeAdder(10).join(",");
const b2 = makeAdder(100).join(",");
const c = over([1, 5, 2, 8, 3], 3);
const d = zipAdd([1, 2, 3], [10, 20, 30]);
const e = scale([1, 2, 3], 5);
const g = rolling([0, 0, 0, 0, 0], [10, 20, 30]);
const h = sumPlus([1, 2, 3], 100);
const i = joinSep(["a", "b", "c"], "-");
const j = reduceMul([1, 2, 3], 10);

describe("closure capture in array methods (#195)", () => {
  test("scalar capture in map", () => expect(a).toBe("11,12,13"));
  test("per-activation capture A", () => expect(b1).toBe("11,12,13"));
  test("per-activation capture B (independent)", () => expect(b2).toBe("101,102,103"));
  test("threshold capture in filter", () => expect(c).toBe("5,8"));
  test("array capture with dynamic index", () => expect(d).toBe("11,22,33"));
  test("factor capture in map", () => expect(e).toBe("5,10,15"));
  test("array capture .length + modular index", () => expect(g).toBe("10,20,30,10,20"));
  test("reduce with scalar capture + init", () => expect(`${h}`).toBe("306"));
  test("reduce string concat with capture", () => expect(i).toBe("-a-b-c"));
  test("reduce no-init with capture", () => expect(`${j}`).toBe("51"));
});
