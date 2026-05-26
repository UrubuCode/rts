import { describe, test, expect } from "rts:test";

// Regression (#195 parcial): arrow retornada captura variáveis livres POR
// VALOR (via bound_args em Entry::Function), com semântica por-ativação —
// curry/closures independentes não compartilham estado. Antes, o mecanismo
// promote-to-global compartilhava o estado entre todas as chamadas.

// curry com captura de string (handle)
const greeter = (greeting: string) => (name: string) => greeting + ", " + name;
const hi = greeter("Hi");
const r1 = hi("Alice");
const r2 = hi("Bob");

// duas ativações independentes — não se atropelam (prova de por-ativação)
const tagger = (tag: string) => (s: string) => "[" + tag + "] " + s;
const err = tagger("ERR");
const info = tagger("INFO");
const e1 = err("boom");
const i1 = info("ok");
// chamar err de novo não foi afetado por info
const e2 = err("again");

describe("closure capture by value", () => {
  test("curry captura string", () => expect(r1).toBe("Hi, Alice"));
  test("curry reusa mesma captura", () => expect(r2).toBe("Hi, Bob"));
  test("ativação independente A", () => expect(e1).toBe("[ERR] boom"));
  test("ativação independente B", () => expect(i1).toBe("[INFO] ok"));
  test("ativação A não corrompida por B", () => expect(e2).toBe("[ERR] again"));
});
