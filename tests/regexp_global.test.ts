import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const re = new RegExp("hello");
const testMatch   = re.test("say hello world");
const testNoMatch = re.test("goodbye");
print(testMatch    ? "test_ok"    : "test_fail");
print(!testNoMatch ? "no_match_ok" : "no_match_fail");

const reI = new RegExp("HELLO", "i");
const flagsOk = reI.test("say hello");
print(flagsOk ? "flags_ok" : "flags_fail");

// exec() — JS spec: retorna Array com captures, primeiro elem eh o match.
const m1: any = re.exec("say hello world");
print(m1 && m1[0] === "hello" ? "exec_ok" : "exec_fail");

const m2 = re.exec("goodbye");
const execNullOk = !m2;
print(execNullOk ? "exec_null_ok" : "exec_null_fail");

const srcOk = re.source === "hello";
print(srcOk ? "source_ok" : "source_fail");

describe("regexp_global", () => {
  test("test() — match positivo",  () => expect(testMatch).toBe(true));
  test("test() — match negativo",  () => expect(testNoMatch).toBe(false));
  test("flag 'i' — case-insensitive", () => expect(flagsOk).toBe(true));
  test("exec() — null/0 sem match",   () => expect(execNullOk).toBe(true));
  test("source — padrao original",    () => expect(srcOk).toBe(true));

  test("saida capturada completa", () =>
    expect(out).toBe(
      "test_ok\nno_match_ok\nflags_ok\nexec_ok\nexec_null_ok\nsource_ok\n"
    ));
});
