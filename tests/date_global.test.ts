import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const ms = Date.now();
print(ms > 0 ? "now_ok" : "now_fail");

const d = new Date();
const dTime = d.getTime();
print(dTime > 0 ? "getTime_ok" : "getTime_fail");

const d2 = new Date(1_000_000_000_000);
const yr  = d2.getFullYear();
const mon = d2.getMonth();
print(yr === 2001 ? "getFullYear_ok" : "getFullYear_fail");
print(mon === 8 ? "getMonth_ok" : "getMonth_fail");

const parsed = Date.parse("2001-09-09T01:46:40.000Z");
print(parsed === 1_000_000_000_000 ? "parse_ok" : "parse_fail");

const iso = d2.toISOString();
const isoOk = iso === "2001-09-09T01:46:40.000Z";
print(isoOk ? "toISOString_ok" : "toISOString_fail");

describe("date_global", () => {
  test("Date.now() retorna timestamp positivo", () =>
    expect(ms > 0).toBe(true));

  test("new Date() — getTime > 0", () =>
    expect(dTime > 0).toBe(true));

  test("new Date(ms) — getFullYear", () =>
    expect(yr === 2001).toBe(true));

  test("new Date(ms) — getMonth", () =>
    expect(mon === 8).toBe(true));

  test("Date.parse() — milissegundos exatos", () =>
    expect(parsed === 1_000_000_000_000).toBe(true));

  test("toISOString() — formato ISO 8601", () =>
    expect(isoOk).toBe(true));

  test("saida capturada completa", () =>
    expect(out).toBe(
      "now_ok\ngetTime_ok\ngetFullYear_ok\ngetMonth_ok\nparse_ok\ntoISOString_ok\n"
    ));
});
