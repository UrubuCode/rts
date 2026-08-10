// node:module — static introspection subset.
// NOTA: `pid`, `platform`, `arch`, `version(s)`, `argv`, `execPath`, `title`,
// `env`, `http.METHODS` e `module.builtinModules` sao PROPRIEDADES no Node, nao
// funcoes — `typeof process.pid` e `number`. Este ficheiro chamava-as com `()`,
// herdado do motor antigo que fizera tudo funcao; num Node real isso lanca
// "pid is not a function". O motor novo escolheu a convencao certa e a fixture
// e que estava errada. Verificado a correr node.
import { describe, test, expect } from "rts:test";
import { isBuiltin, builtinModules, wrap, syncBuiltinESMExports } from "node:module";

const fsB = isBuiltin("fs");
const nodeFsB = isBuiltin("node:fs");
const nodeSqliteB = isBuiltin("node:sqlite");
const bareSqliteB = isBuiltin("sqlite"); // prefix-only → false bare
const bogusB = isBuiltin("totally-not-a-core-module");

const mods = builtinModules;
const hasFs = mods.indexOf("fs") >= 0;
const hasPrefixOnly = mods.indexOf("node:sqlite") >= 0;
const modsNonEmpty = mods.length > 10;

const wrapped = wrap("const x = 1;");
const wrapStartsOk = wrapped.indexOf("(function (exports, require, module, __filename, __dirname)") === 0;
const wrapHasBody = wrapped.indexOf("const x = 1;") >= 0;
const wrapEndsOk = wrapped.indexOf("});") >= 0;

syncBuiltinESMExports(); // no-op, must not throw
const syncOk = true;

describe("node:module", () => {
    test("isBuiltin fs", () => expect(fsB).toBe(true));
    test("isBuiltin node:fs", () => expect(nodeFsB).toBe(true));
    test("isBuiltin node:sqlite", () => expect(nodeSqliteB).toBe(true));
    test("isBuiltin bare sqlite false", () => expect(bareSqliteB).toBe(false));
    test("isBuiltin bogus false", () => expect(bogusB).toBe(false));
    test("builtinModules has fs", () => expect(hasFs).toBe(true));
    test("builtinModules has node:sqlite", () => expect(hasPrefixOnly).toBe(true));
    test("builtinModules non-empty", () => expect(modsNonEmpty).toBe(true));
    test("wrap prefix", () => expect(wrapStartsOk).toBe(true));
    test("wrap body", () => expect(wrapHasBody).toBe(true));
    test("wrap suffix", () => expect(wrapEndsOk).toBe(true));
    test("syncBuiltinESMExports no-op", () => expect(syncOk).toBe(true));
});
