import { describe, test, expect } from "rts:test";

// (#777 follow-up) `obj.missing === undefined` em Member access deve
// retornar true. Antes, MAP_GET_CHAIN retornava 0 para key ausente
// (correto), mas o operator `=== undefined` so' aceitava sentinels
// MIN+2/MIN+4. Como Member access pode retornar 0 ou sentinel sem o
// codegen saber, o operator agora considera 0 tambem como
// undefined-equivalente quando o LHS eh Member/OptChain.

const o: any = Object.create(null);
const eq1 = o.toString === undefined;
const ne1 = o.toString !== undefined;

const o2: any = {};
const eq2 = o2.notExist === undefined;

const o3: any = { x: 1 };
const eq3 = o3.x === undefined;
const ne3 = o3.x !== undefined;
const eq4 = o3.missing === undefined;

describe("Member access === undefined (#777 follow-up)", () => {
  test("Object.create(null).toString === undefined", () => expect(eq1).toBe(true));
  test("!== undefined ne1", () => expect(ne1).toBe(false));
  test("{}.notExist === undefined", () => expect(eq2).toBe(true));
  test("{x:1}.x !== undefined (existente)", () => expect(ne3).toBe(true));
  test("{x:1}.missing === undefined", () => expect(eq4).toBe(true));
  test("{x:1}.x existente nao bate como undefined", () =>
    expect(eq3).toBe(false));
});
