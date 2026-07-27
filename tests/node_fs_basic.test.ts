// (#287) node:fs basic — readFileSync, writeFileSync, existsSync.

import { describe, test, expect } from "rts:test";
import { env } from "rts";
import {
    readFileSync,
    writeFileSync,
    existsSync,
    appendFileSync,
} from "node:fs";
import { Buffer } from "node:buffer";

// `readFileSync(path)` SEM encoding devolve um Buffer (bytes), nao uma string —
// e o contrato do Node, e o RTS o segue. A versao anterior deste teste anotava
// `: string` e comparava com texto, entao recebia "104,101,108,..." e falhava.
// Duas formas corretas, ambas exercitadas abaixo:
//   • `readFileSync(path, "utf8")` → string, decodificada na leitura;
//   • `Buffer.toString(buf)`       → decodifica um Buffer ja lido.
//
// DIVERGENCIA CONHECIDA do Node, declarada e nao contornada: em Node vale
// `buf.toString()` (metodo de INSTANCIA) e `String(buf)`; aqui a decodificacao
// se pede como ESTATICA, `Buffer.toString(buf)`. O motivo esta em
// `crates/rts-node/src/buffer/mod.rs`: o receptor de um Buffer e um
// `Entry::Vec` sem tag de classe provada, entao `buf.toString()` resolve para
// `Array.prototype.toString` (junta com virgula) antes de alcancar qualquer
// override de Buffer. Corrigir exige rastrear `JsKind::Buffer` no Lowerer — uma
// linha `RecvClass::Buffer` analoga a `RecvClass::Array` —, que e trabalho de
// motor e esta fora do escopo deste teste.

const tmp = env.get_var("TEMP") || "/tmp";

const path = tmp + "/rts_node_fs_test.txt";
writeFileSync(path, "hello from rts");
// forma 1: encoding na leitura → string direto.
const content: string = readFileSync(path, "utf8");
// forma 2: le bytes e decodifica depois.
const contentBuf = readFileSync(path);
const contentFromBuf: string = Buffer.toString(contentBuf);
const exists = existsSync(path);

const path2 = tmp + "/rts_node_fs_missing.txt";
const exists2 = existsSync(path2);

// append
const appendPath = tmp + "/rts_node_fs_append.txt";
writeFileSync(appendPath, "line1\n");
appendFileSync(appendPath, "line2\n");
const appended: string = readFileSync(appendPath, "utf8");

// roundtrip large-ish
const big = "x".repeat(1000);
writeFileSync(tmp + "/rts_node_fs_big.txt", big);
const bigRead: string = readFileSync(tmp + "/rts_node_fs_big.txt", "utf8");
const bigLen: i64 = bigRead.length;

describe("node_fs_basic", () => {
    test("writeFileSync + readFileSync roundtrip", () =>
        expect(content).toBe("hello from rts"));
    test("readFileSync sem encoding devolve bytes decodificaveis", () =>
        expect(contentFromBuf).toBe("hello from rts"));
    test("existsSync true after write", () => expect(exists).toBe(true));
    test("existsSync false for missing", () => expect(exists2).toBe(false));
    test("appendFileSync appends to existing", () =>
        expect(appended).toBe("line1\nline2\n"));
    test("large content roundtrip length", () =>
        expect(bigLen).toBe(1000));
});
