// node:util — parseArgs.
//
// HISTÓRICO: este arquivo travava (loop infinito) sob `rts test` e foi tratado
// como determinístico — chegou a ser "bissecado" até um limiar aparente (3
// acessos a membro do resultado passavam, 4 travavam). Aquela leitura estava
// ERRADA: o hang era INTERMITENTE. Medido depois: 0 de 12 rodadas do arquivo
// isolado travam, e a suíte completa passa sem ele na lista de travados.
//
// O que ficou estabelecido e continua valendo, para quem vir isto voltar:
//   • quando travava, o processo consumia CPU (loop), não ficava parado;
//   • `rts ir` compilava normalmente → seria em execução, não em compilação;
//   • travava igual com `RTS_GC_DISABLE=1` → não era o coletor;
//   • NÃO foi corrigido de propósito: reverter o fix do `.name` (o único
//     candidato) não trouxe o hang de volta.
//
// A lição de método vale mais que o diagnóstico: uma falha intermitente
// bissecada como se fosse determinística produz um "limiar" que é ruído. Antes
// de bissecar, medir a taxa de reprodução.
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
