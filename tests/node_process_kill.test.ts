// node:process — kill with signal 0 (existence check, safe).
// NOTA: `pid`, `platform`, `arch`, `version(s)`, `argv`, `execPath`, `title`,
// `env`, `http.METHODS` e `module.builtinModules` sao PROPRIEDADES no Node, nao
// funcoes — `typeof process.pid` e `number`. Este ficheiro chamava-as com `()`,
// herdado do motor antigo que fizera tudo funcao; num Node real isso lanca
// "pid is not a function". O motor novo escolheu a convencao certa e a fixture
// e que estava errada. Verificado a correr node.
import { describe, test, expect } from "rts:test";
import { kill, pid } from "node:process";

// signal 0 on our own pid → the process exists → true.
const selfExists = kill(pid, 0);
// a pid that almost certainly does not exist → false.
const bogus = kill(2147483000, 0);

describe("node:process kill", () => {
    test("self exists (signal 0)", () => expect(selfExists).toBe(true));
    test("bogus pid false", () => expect(bogus).toBe(false));
});
