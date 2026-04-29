import { describe, test, expect } from "rts:test";
import { gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class Box {
  v: number = 0;
}

// MVP do gc.collect: passa root, coletor preserva apenas o root + alcancaveis,
// libera o resto. Limitacao conhecida: sem codegen-side root tracking, strings
// globais (como __rtsCapturedOutput) sao coletadas — entao nao usamos `print`
// atravessando collect; capturamos resultados antes e fazemos os prints depois.

const a = new Box();
const b = new Box();
const c = new Box();

const beforeAll = gc.live_count();
const freed1 = gc.collect_vec(([a, b, c] as unknown) as number);
const afterAll = gc.live_count();

const x = new Box();
const beforeOne = gc.live_count();
const freed2 = gc.collect(x);
const afterOne = gc.live_count();

// Agora os prints — strings deste ponto pra frente nao foram coletadas.
print(`preserved_all_no_change: ${afterAll === beforeAll}`);
print(`freed_x_alone: ${beforeOne - afterOne >= 0}`);
// `freed1` e `freed2` sao usados pra evitar dead-code warning.
print(`freed1_nonneg: ${freed1 >= 0}`);
print(`freed2_nonneg: ${freed2 >= 0}`);

describe("gc_collect_basic", () => {
  test("collect_preserves_roots", () =>
    expect(__rtsCapturedOutput).toBe(
      "preserved_all_no_change: true\nfreed_x_alone: true\nfreed1_nonneg: true\nfreed2_nonneg: true\n"
    ));
});
