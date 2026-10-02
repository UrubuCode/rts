// Como um native lê um argumento de TEXTO — #2850.
//
// Três módulos partilham uma convenção escrita três vezes (`url/mod.rs::text`,
// `path.rs::text`, `querystring.rs::argument_text`) que decide "`undefined` é
// ausente" e responde `None`. Isso é certo num sítio e errado em dez, porque
// são TRÊS perguntas diferentes e não uma:
//
//   1. parâmetro `USVString` OBRIGATÓRIO (WebIDL: URL, URLSearchParams,
//      TextEncoder, Blob) — coage sempre, `undefined` incluído, dá "undefined";
//   2. parâmetro que VALIDA o tipo (path.*, querystring.parse) — recusa um
//      não-string com ERR_INVALID_ARG_TYPE;
//   3. parâmetro OPCIONAL com default (console.time) — `undefined` e ausente
//      usam ambos o default.
//
// Cada linha abaixo foi medida em `node` 22 e o valor esperado é o dele.
import { describe, test, expect } from "rts:test";
import { basename, join, resolve } from "node:path";
import { escape as qsEscape, parse as qsParse } from "node:querystring";

function answer(f: () => unknown): string {
    try {
        const v = f();
        return typeof v === "string" ? JSON.stringify(v) : String(v);
    } catch (e: any) {
        return "THREW " + (e.code || e.constructor.name);
    }
}

// ---- 1. USVString obrigatório: coage, undefined incluído.
const urlUndefined = answer(() => new URL(undefined as any, "http://h/").href);
const urlNumber = answer(() => new URL(7 as any, "http://h/").href);
const paramsGet = answer(() => new URLSearchParams("undefined=hit").get(undefined as any));
const paramsHas = answer(() => new URLSearchParams("undefined=hit").has(undefined as any));
const qsEscapeUndefined = answer(() => qsEscape(undefined as any));

// ---- 2. Valida o tipo: recusa um não-string.
const basenameUndefined = answer(() => basename(undefined as any));
const joinNumbers = answer(() => join(7 as any, 8 as any));
const resolveUndefined = answer(() => resolve(undefined as any));
const qsParseNumber = answer(() => JSON.stringify(qsParse(7 as any)));

// ---- 3. Opcional com default: NAO esta aqui, e a razao fica escrita.
// `console.time()` imprime `undefined:` onde o Node imprime `default:`, e isso
// nao e assertavel daqui: o label vive na linha impressa, e capturar stdout
// substituindo `process.stdout.write` mede a sonda e nao o motor — foi
// exatamente o que me deu uma medicao errada ao diagnosticar #2850. Fica
// corrigido no codigo e medido a mao, sem um teste que passa com o defeito
// presente.

// ---- Controlos: o que já acerta, para que a correcção não o leve consigo.
const eventNumber = answer(() => new (globalThis as any).Event(7).type);
const decoderUndefined = answer(() => new (globalThis as any).TextDecoder(undefined).encoding);
const encoderEmpty = answer(() => new (globalThis as any).TextEncoder().encode().length);
const pushUndefined = answer(() => { const a = [1]; a.push(undefined); return a.length; });
const mathMaxUndefined = answer(() => Math.max(undefined as any));

describe("a USVString parameter coerces, undefined included (#2850)", () => {
    test("new URL(undefined, base)", () => expect(urlUndefined).toBe('"http://h/undefined"'));
    test("new URL(7, base) — already right", () => expect(urlNumber).toBe('"http://h/7"'));
    test("params.get(undefined) finds the key named undefined", () => expect(paramsGet).toBe('"hit"'));
    test("params.has(undefined)", () => expect(paramsHas).toBe("true"));
    test("querystring.escape(undefined)", () => expect(qsEscapeUndefined).toBe('"undefined"'));
});

describe("a type-validating parameter refuses a non-string (#2850)", () => {
    test("path.basename(undefined)", () => expect(basenameUndefined).toBe("THREW ERR_INVALID_ARG_TYPE"));
    test("path.join(7, 8)", () => expect(joinNumbers).toBe("THREW ERR_INVALID_ARG_TYPE"));
    test("path.resolve(undefined)", () => expect(resolveUndefined).toBe("THREW ERR_INVALID_ARG_TYPE"));
    test("querystring.parse(7) is empty, not a key named 7", () => expect(qsParseNumber).toBe('"{}"'));
});

describe("what already read a text argument correctly", () => {
    test("new Event(7).type", () => expect(eventNumber).toBe('"7"'));
    test("new TextDecoder(undefined).encoding", () => expect(decoderUndefined).toBe('"utf-8"'));
    test("new TextEncoder().encode() is empty", () => expect(encoderEmpty).toBe("0"));
    test("[1].push(undefined) pushes one — the arity IS known", () => expect(pushUndefined).toBe("2"));
    test("Math.max(undefined) is NaN", () => expect(mathMaxUndefined).toBe("NaN"));
});
