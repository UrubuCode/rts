import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `inst.method().filter(...)` em CHAIN DIRETO
// (sem var) onde o metodo de classe declara return array crashava SIGILL.
// looks_array_call nao reconhecia metodos de usuario com ret-array. Fix:
// thread-local METHODS_RET_ARRAY (pre-scan textual do AST) consultado em
// looks_array_call. Complementa #1241 (que cobriu o caso com var).

let out = "";
function print(v: string): void { out += v + "\n"; }

class Store {
  items: string[] = [];
  addAll(xs: string[]): void { for (const x of xs) this.items.push(x); }
  getItems(): string[] { return this.items; }
}

const st = new Store();
st.addAll(["a", "b", "c"]);

// chain direto sobre metodo ret-array
print(st.getItems().filter(i => i !== "b").join(""));   // ac
print(st.getItems().map(i => i.toUpperCase()).join("")); // ABC

describe("method ret array chain direct", () => {
  test("chain direto sobre metodo ret-array nao crasha", () =>
    expect(out).toBe("ac\nABC\n"));
});
