import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Decorator factory: @decorator(arg) recebe args em compile-time.

function entity(name: string): i64 {
  print("registrando entidade: " + name);
  return 0;
}

@entity("usuario")
class User {
  hi(): void { print("usuario.hi"); }
}

@entity("produto")
class Product {
  hi(): void { print("produto.hi"); }
}

new User().hi();
new Product().hi();

describe("fixture:decorator_factory", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("registrando entidade: usuario\nregistrando entidade: produto\nusuario.hi\nproduto.hi\n");
  });
});
