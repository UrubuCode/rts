import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// satisfies em assignment direto
const cfgA = { port: 8080, host: 3000 } satisfies { port: number; host: number };
print(`portA=${cfgA.port}`);
print(`hostA=${cfgA.host}`);

// as const em object literal — manter map_get
const cfgB = { count: 42 } as const;
print(`countB=${cfgB.count}`);

// as Type em object literal
const cfgC = { value: 999 } as { value: number };
print(`valueC=${cfgC.value}`);

// non-null assertion peelando object literal
const cfgD = ({ score: 7 } satisfies { score: number })!;
print(`scoreD=${cfgD.score}`);

// satisfies em template literal direto sem alias
print(`direct=${({ a: 11 } satisfies { a: number }).a}`);

describe("ts_satisfies_variations", () => {
  test("captura todas as variacoes", () => expect(__rtsCapturedOutput).toBe(
    "portA=8080\nhostA=3000\ncountB=42\nvalueC=999\nscoreD=7\ndirect=11\n"
  ));
});
