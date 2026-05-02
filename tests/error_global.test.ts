import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const e = new Error("something went wrong");
const eMsgOk  = e.message === "something went wrong";
const eNameOk = e.name === "Error";
const eToStrOk = e.toString() === "Error: something went wrong";
print(eMsgOk  ? "message_ok"  : "message_fail");
print(eNameOk ? "name_ok"     : "name_fail");
print(eToStrOk ? "tostring_ok" : "tostring_fail");

const te = new TypeError("bad type");
const teNameOk = te.name === "TypeError";
const teMsgOk  = te.message === "bad type";
print(teNameOk ? "type_error_name_ok" : "type_error_name_fail");
print(teMsgOk  ? "type_error_msg_ok"  : "type_error_msg_fail");

const re = new RangeError("out of range");
const reNameOk = re.name === "RangeError";
print(reNameOk ? "range_error_ok" : "range_error_fail");

const se = new SyntaxError("unexpected token");
const seNameOk = se.name === "SyntaxError";
print(seNameOk ? "syntax_error_ok" : "syntax_error_fail");

const e2 = new Error("");
const e2ToStrOk = e2.toString() === "Error";
print(e2ToStrOk ? "empty_msg_ok" : "empty_msg_fail");

describe("error_global", () => {
  test("Error — message", () => expect(eMsgOk).toBe(true));
  test("Error — name",    () => expect(eNameOk).toBe(true));
  test("Error — toString", () => expect(eToStrOk).toBe(true));

  test("TypeError — name e message", () => {
    expect(teNameOk).toBe(true);
    expect(teMsgOk).toBe(true);
  });

  test("RangeError — name",   () => expect(reNameOk).toBe(true));
  test("SyntaxError — name",  () => expect(seNameOk).toBe(true));
  test("Error vazio — toString retorna so name", () => expect(e2ToStrOk).toBe(true));

  test("saida capturada completa", () =>
    expect(out).toBe(
      "message_ok\nname_ok\ntostring_ok\n" +
      "type_error_name_ok\ntype_error_msg_ok\n" +
      "range_error_ok\nsyntax_error_ok\nempty_msg_ok\n"
    ));
});
