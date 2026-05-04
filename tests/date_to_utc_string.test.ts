import { describe, test, expect } from "rts:test";

// (#552) Date.toUTCString deve retornar formato RFC 1123, nao ISO 8601.

const epoch = new Date(0).toUTCString();
const y2k = new Date(946684800000).toUTCString(); // 2000-01-01T00:00:00Z

describe("date_to_utc_string", () => {
  test("epoch (#552)", () => expect(epoch).toBe("Thu, 01 Jan 1970 00:00:00 GMT"));
  test("y2k (#552)", () => expect(y2k).toBe("Sat, 01 Jan 2000 00:00:00 GMT"));
});
