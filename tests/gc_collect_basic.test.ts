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
// (gc.collect_vec foi removido — superficie legacy do motor antigo, gc-new-api-plan
// C2; sobra a API de root unico `collect` + `live_count`.)

const x = new Box();
const beforeOne = gc.live_count();
const freed = gc.collect(x);
const afterOne = gc.live_count();

// HandleTable e' state global compartilhado entre fixtures no test runner —
// comparamos apenas que o coletor *roda* (freed >= 0, live_count >= 0) sem
// travar/segfault. Validacao de semantica fica em `cargo test` unit do collector.
print(`live_before_works: ${beforeOne >= 0}`);
print(`live_after_works: ${afterOne >= 0}`);
print(`freed_nonneg: ${freed >= 0}`);

describe("gc_collect_basic", () => {
  test("collect_runs_without_crashing", () =>
    expect(__rtsCapturedOutput).toBe(
      "live_before_works: true\nlive_after_works: true\nfreed_nonneg: true\n"
    ));
});
