import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const v = inst.method(); v.map(...)` onde o
// metodo de classe declara return array (`getItems(): string[]`) crashava
// SIGILL. collect_array_receiver_idents nao sabia que o metodo retorna array,
// entao a var nao era registrada -> `.map()` nao roteado como array method.
// Fix: pre-scan dos metodos de classe com return_type array (lido textual do
// AST, sem precisar do ABI compilado) e marca `inst.method()` como
// array-returning no collect.

let out = "";
function print(v: string): void { out += v + "\n"; }

class Store {
  items: string[] = [];
  addAll(xs: string[]): void { for (const x of xs) this.items.push(x); }
  getItems(): string[] { return this.items; }
  tags(): Array<string> { return this.items; }
}

const st = new Store();
st.addAll(["a", "b", "c"]);

const items = st.getItems();
print(items.map(i => i.toUpperCase()).join(""));   // ABC

const f = st.getItems();
print(f.filter(i => i !== "b").join(""));           // ac

// Array<T> tambem qualifica
const t = st.tags();
print(t.map(x => x + "!").join(""));                // a!b!c!

describe("method ret array var map", () => {
  test("var de metodo ret-array eh reconhecida", () =>
    expect(out).toBe("ABC\nac\na!b!c!\n"));
});
