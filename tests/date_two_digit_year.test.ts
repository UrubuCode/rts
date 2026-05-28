import { describe, test, expect } from "rts:test";

// JS legacy: anos 0..=99 em Date.UTC(y,...) e new Date(y,m,...) sao
// interpretados como 1900+y. Anos >= 100 ficam literais.
const y5 = new Date(Date.UTC(5, 0, 1)).toISOString();
const y99 = new Date(Date.UTC(99, 0, 1)).toISOString();
const y0 = new Date(Date.UTC(0, 0, 1)).toISOString();
const y100 = new Date(Date.UTC(100, 0, 1)).toISOString();
const y1999 = new Date(Date.UTC(1999, 0, 1)).toISOString();
const utc70 = Date.UTC(70, 0, 1);
const fy50 = new Date(Date.UTC(50, 5, 15)).getUTCFullYear();
const y2024 = new Date(Date.UTC(2024, 0, 1)).toISOString();

describe("Date two-digit year (1900+y)", () => {
  test("Date.UTC(5) -> 1905", () => expect(y5).toBe("1905-01-01T00:00:00.000Z"));
  test("Date.UTC(99) -> 1999", () => expect(y99).toBe("1999-01-01T00:00:00.000Z"));
  test("Date.UTC(0) -> 1900", () => expect(y0).toBe("1900-01-01T00:00:00.000Z"));
  test("Date.UTC(100) literal", () => expect(y100).toBe("0100-01-01T00:00:00.000Z"));
  test("Date.UTC(1999) literal", () => expect(y1999).toBe("1999-01-01T00:00:00.000Z"));
  test("Date.UTC(70) == epoch 1970", () => expect(utc70).toBe(0));
  test("getUTCFullYear de 50 -> 1950", () => expect(fy50).toBe(1950));
  test("Date.UTC(2024) literal", () => expect(y2024).toBe("2024-01-01T00:00:00.000Z"));
});
