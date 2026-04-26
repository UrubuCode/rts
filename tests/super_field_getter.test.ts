import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.x quando há getter override em Sub: super pula o override
// e invoca o getter do Base diretamente.

class Base {
    _x: number = 100;
    get x(): number {
        return this._x; // 100
    }
}

class Sub extends Base {
    get x(): number {
        return 999; // override que NÃO deve ser chamado por super.x
    }

    fromSuper(): number {
        return super.x; // chama Base.get x → 100
    }

    fromThis(): number {
        return this.x; // virtual → Sub.get x → 999
    }
}

const s = new Sub();
const h1 = gc.string_from_i64(s.fromSuper());
print(h1); gc.string_free(h1); // 100

const h2 = gc.string_from_i64(s.fromThis());
print(h2); gc.string_free(h2); // 999

describe("fixture:super_field_getter", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n999\n");
  });
});
