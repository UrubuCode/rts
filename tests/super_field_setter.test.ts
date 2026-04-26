import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.x = v quando há setter override em Sub: super pula o override.

class Base {
    _x: number = 0;
    set x(v: number) {
        this._x = v;
    }
}

class Sub extends Base {
    set x(v: number) {
        // override que NÃO deve ser chamado por super.x = ...
        this._x = v * 100;
    }

    setBase(v: number): void {
        super.x = v; // chama Base.set x → _x = v
    }

    setThis(v: number): void {
        this.x = v; // virtual → Sub.set x → _x = v * 100
    }
}

const s = new Sub();
s.setBase(7);
const h1 = gc.string_from_i64(s._x);
print(h1); gc.string_free(h1); // 7

s.setThis(7);
const h2 = gc.string_from_i64(s._x);
print(h2); gc.string_free(h2); // 700

describe("fixture:super_field_setter", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("7\n700\n");
  });
});
