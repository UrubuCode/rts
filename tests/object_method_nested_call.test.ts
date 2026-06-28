import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#261) Method de um obj chamando method de outro obj. O `this` de cada method
// referencia seu próprio receiver — nesting não quebra. Lê campos via `this.marker`
// (via canônica JS; antes usava o escape hatch `collections.map_get(this, ...)`).

const inner: any = {
  marker: 99,
  read(): number {
    return this.marker;
  },
};

const outer: any = {
  marker: 7,
  combo(): number {
    const myMarker: number = this.marker;
    const innerMarker: number = inner.read();
    return myMarker * 100 + innerMarker;
  },
};

const r: number = outer.combo();
print("r=" + r);  // 7*100 + 99 = 799

// Confirma que outer ainda eh ele mesmo apos call retornar
const m: number = outer.marker;
print("m=" + m);

describe("method nested call (#261)", () => {
  test("stack this slot preserva receiver apos nested invoke", () =>
    expect(__rtsCapturedOutput).toBe(
      "r=799\n" +
      "m=7\n"
    ));
});
