import { describe, test, expect } from "rts:test";

// (#383) Identifier global desconhecido eh hard-fail no compile time.
// Antes da issue: warning + segfault em runtime.
// Apos fix #383: erro de compilacao impedindo o build do programa.
//
// Como o teste roda dentro do codegen, nao podemos referenciar a
// fn desconhecida diretamente — o programa nao chegaria a rodar.
// Em vez disso, validamos uma propriedade do contrato: ident
// declarado existe e nao gera erro.
//
// Cobertura do "tem que falhar ao compilar" fica no
// tests/undefined_ident_hard_error.test.ts (que precisa de outra
// infra pra capturar o erro do build).

const known: i64 = 42;
const reads = known + 0;

describe("undefined_ident_hard_fail_documented", () => {
  test("known identifier compiles fine", () =>
    expect(reads).toBe(42));
});
