import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Decorator factory: `@entity("name")` is TWO calls. `entity("name")` runs
// where it is written, for its arguments, and the function it RETURNS is the
// decorator — it is called with the class.
//
// This fixture asserted the opposite until 2026-10-03. It had `entity` return
// `0` and expected the class to pass through undecorated, and the engine was
// written to match it: a decorator spelled as a call had the call evaluated and
// the result discarded. So the fixture was green and NO decorator in the
// ordinary spelling ever ran — the fixture was the defect's alibi.
//
// It is corrected rather than deleted, and the correction is what bun 1.4.0
// does with `experimentalDecorators`. The old form is not merely unsupported
// there, it THROWS: `entity("usuario")` answers `0`, and `0` is then called
// with the class.

function entity(name: string) {
  print("registering entity: " + name);
  return (target: any) => {
    print("applied to: " + name);
    return target;
  };
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
    expect(__rtsCapturedOutput).toBe(
      "registering entity: usuario\napplied to: usuario\n" +
      "registering entity: produto\napplied to: produto\n" +
      "usuario.hi\nproduto.hi\n",
    );
  });
});
