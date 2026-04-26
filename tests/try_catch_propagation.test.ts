import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// throw dentro de fn deve propagar até o try/catch do caller.
// Nota: fase 1 não tem unwind real — o body continua até o ponto de
// observação (try/catch ou call site). Este teste exercita o caso onde
// o body da fn imprime ANTES do throw, e o caller observa o erro.

function inner(): void {
    print("inner-before");
    throw "boom";
    // statements após throw na fase 1 ainda executam — mas evitamos
    // testar isso; aqui o body é puro até o throw.
}

try {
    inner();
} catch (e) {
    print(`caught: ${e}`);
}

print("end");

describe("fixture:try_catch_propagation", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("inner-before\ncaught: boom\nend\n");
  });
});
