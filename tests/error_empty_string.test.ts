import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #452: tambem cobrir new Error("") com literal vazio explicito
// (caminho diferente: passa handle de string vazia em vez de omitir arg).

const e1 = new Error("");
print(`m1=[${e1.message}]`);

// Em try/catch
try {
  throw new Error("");
} catch (e) {
  print(`m2=[${(e as Error).message}]`);
}

// Em fn user
function fail(): void {
  throw new TypeError("");
}
try {
  fail();
} catch (e) {
  print(`m3=[${(e as TypeError).message}]`);
  print(`n3=${(e as TypeError).name}`);
}

describe("error com message vazia explicita (#452)", () => {
  test("new Error('') / throw new Error('') / TypeError('')", () => {
    expect(__rtsCapturedOutput).toBe(
      "m1=[]\n" +
      "m2=[]\n" +
      "m3=[]\nn3=TypeError\n"
    );
  });
});
