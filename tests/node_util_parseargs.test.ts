// node:util — parseArgs.
//
// ⚠ ESTE ARQUIVO TRAVA (loop infinito) sob `rts test`. Diagnóstico do que já foi
// estabelecido, para quem for atacar — o bug é do MOTOR, não do parseArgs:
//
//   • `rts run` do mesmo arquivo termina limpo; só `rts test` trava.
//   • `rts ir` compila normalmente → o loop é em EXECUÇÃO, não em compilação.
//   • O processo consome CPU (loop), não fica parado (não é deadlock).
//   • Trava igual com `RTS_GC_DISABLE=1` → não é o coletor.
//   • Cada bloco isolado passa; o gatilho é o ACÚMULO.
//   • Bissecado até o limiar: partindo dos dois blocos de `parseArgs`, TRÊS
//     acessos a membro do objeto retornado passam e QUATRO travam
//     (`const z = r.values.verbose === true;` repetido).
//   • Não é quantidade de código: SEIS `const z = <int>;` triviais não travam.
//     São especificamente os acessos a MEMBRO do resultado do parseArgs.
//   • Adicionar `io.print` entre os passos faz os 5 testes PASSAREM — o que
//     também aponta para algo sensível a ordem/estado, não a uma construção.
//
// O próximo passo seria instrumentar o caminho de acesso dinâmico a membro
// (`lower_dynamic_get_expr` / `__rtsadp_obj_get`) para ver qual laço não
// termina a partir do 4º acesso.
import { describe, test, expect } from "rts:test";
import { parseArgs } from "node:util";

// boolean + string long options + positionals.
const cfg = {
    args: ["--verbose", "--name", "alice", "file1.txt", "file2.txt"],
    options: {
        verbose: { type: "boolean" },
        name: { type: "string" },
    },
    allowPositionals: true,
};
const r = parseArgs(cfg);
const verboseOk = r.values.verbose === true;
const nameOk = r.values.name === "alice";
const posOk = r.positionals.length === 2 && r.positionals[0] === "file1.txt";

// short options + --flag=value form.
const cfg2 = {
    args: ["-v", "--out=result.txt"],
    options: {
        verbose: { type: "boolean", short: "v" },
        out: { type: "string" },
    },
    allowPositionals: false,
};
const r2 = parseArgs(cfg2);
const shortOk = r2.values.verbose === true;
const inlineOk = r2.values.out === "result.txt";

describe("node:util parseArgs", () => {
    test("boolean flag", () => expect(verboseOk).toBe(true));
    test("string option", () => expect(nameOk).toBe(true));
    test("positionals", () => expect(posOk).toBe(true));
    test("short alias", () => expect(shortOk).toBe(true));
    test("--key=value form", () => expect(inlineOk).toBe(true));
});
