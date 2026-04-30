import { describe, test, expect } from "rts:test";
import { gc, io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #300: catch (e: Error) deve popular local_class_ty para que e.message
// roteie via __RTS_FN_GL_ERROR_MESSAGE em vez de map_get_static.
// Antes: e.message retornava 0 (handle invalido).

// Caso 1: anotacao explicita
try {
  throw new Error("explicit-anno");
} catch (e: Error) {
  print(e.message);
}

// Caso 2: inferencia sem anotacao (todos os throws sao new Error)
try {
  throw new Error("inferred");
} catch (e) {
  print(e.message);
}

// Caso 3: throw em fn chamada — propagacao via thread-local error
function boom(): void {
  throw new Error("from-fn");
}
try {
  boom();
} catch (e: Error) {
  print(e.message);
}

// Caso 4: TypeError tambem (subclasse de Error)
try {
  throw new TypeError("type-err");
} catch (e: TypeError) {
  print(e.message);
}

describe("fixture:catch_error_message", () => {
  test("catch (e: Error) routes e.message correctly (#300)", () => {
    expect(__rtsCapturedOutput).toBe(
      "explicit-anno\ninferred\nfrom-fn\ntype-err\n"
    );
  });
});
