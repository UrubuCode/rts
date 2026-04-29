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

// Agora os prints. Como HandleTable e' state global compartilhado entre
// fixtures no test runner, comparamos apenas que o coletor *roda* (returna
// freed >= 0) sem travar/segfault. Validacao de semantica precisa fica em
// `cargo test` unit do collector.
print(`live_works: ${beforeAll >= 0}`);
print(`live_after_collect_vec_works: ${afterAll >= 0}`);
print(`live_before_one_works: ${beforeOne >= 0}`);
print(`live_after_one_works: ${afterOne >= 0}`);
print(`freed1_nonneg: ${freed1 >= 0}`);
print(`freed2_nonneg: ${freed2 >= 0}`);

describe("gc_collect_basic", () => {
  test("collect_runs_without_crashing", () =>
    expect(__rtsCapturedOutput).toBe(
      "live_works: true\nlive_after_collect_vec_works: true\nlive_before_one_works: true\nlive_after_one_works: true\nfreed1_nonneg: true\nfreed2_nonneg: true\n"
    ));
});
