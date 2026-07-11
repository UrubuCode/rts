// node:http — methods/status registry + header validators (sync surface).
import { describe, test, expect } from "rts:test";
import { METHODS, STATUS_CODES, validateHeaderName, validateHeaderValue } from "node:http";

const methods = METHODS();
const hasGet = methods.indexOf("GET") >= 0 && methods.indexOf("POST") >= 0;
const methodsNonEmpty = methods.length > 10;

const codes = STATUS_CODES();
const ok200 = codes["200"] === "OK";
const nf404 = codes["404"] === "Not Found";

let nameOkThrew = false;
try { validateHeaderName("Content-Type"); } catch (e) { nameOkThrew = true; }
let nameBadThrew = false;
try { validateHeaderName("Bad Header"); } catch (e) { nameBadThrew = true; } // space invalid

let valueOkThrew = false;
try { validateHeaderValue("X", "hello"); } catch (e) { valueOkThrew = true; }
let valueBadThrew = false;
try { validateHeaderValue("X", "bad\nvalue"); } catch (e) { valueBadThrew = true; } // LF invalid

describe("node:http", () => {
    test("METHODS has GET/POST", () => expect(hasGet).toBe(true));
    test("METHODS non-empty", () => expect(methodsNonEmpty).toBe(true));
    test("STATUS_CODES 200", () => expect(ok200).toBe(true));
    test("STATUS_CODES 404", () => expect(nf404).toBe(true));
    test("validateHeaderName valid ok", () => expect(nameOkThrew).toBe(false));
    test("validateHeaderName invalid throws", () => expect(nameBadThrew).toBe(true));
    test("validateHeaderValue valid ok", () => expect(valueOkThrew).toBe(false));
    test("validateHeaderValue invalid throws", () => expect(valueBadThrew).toBe(true));
});
