import { describe, test, expect } from "rts:test";

// (#98/349) getOwnPropertyDescriptor deve preservar o flag `configurable`
// definido via Object.defineProperty. Regressao introduzida quando o call
// site passou a rotear pela versao _PROXY (forward_get_own_property_descriptor
// hardcodava configurable=true). Cobre writable/enumerable/configurable.
//
// Coletamos tudo numa string no top-level (como o fixture cross-runtime
// 349 faz) — comparar bool sentinels do descriptor diretamente em
// closures test() eh fragil sob GC; a coercao via `"" + flag` eh estavel.
const obj: any = {};
Object.defineProperty(obj, "ro", { value: 42, writable: false, enumerable: true, configurable: false });
Object.defineProperty(obj, "rw", { value: 7, writable: true, enumerable: false, configurable: true });

const dRo = Object.getOwnPropertyDescriptor(obj, "ro")!;
const dRw = Object.getOwnPropertyDescriptor(obj, "rw")!;

const summary: string =
  "ro=" + dRo.value + "," + dRo.writable + "," + dRo.enumerable + "," + dRo.configurable +
  "|rw=" + dRw.writable + "," + dRw.enumerable + "," + dRw.configurable;

describe("descriptor_configurable (#98/349)", () => {
  test("descriptor flags preservados", () =>
    expect(summary).toBe("ro=42,false,true,false|rw=true,false,true"));
});
