import { describe, test, expect } from "rts:test";

// const x = cfg.server (ou cfg?.server) propaga nested types pra
// que x.host retorne tipo correto.

const cfg = { server: { host: "localhost", port: 3000 } };

const s = cfg.server;
const h = s.host;
const p = s.port;

const s2 = cfg?.server;
const h2 = s2?.host;

describe("nested_obj_via_var", () => {
  test("cfg.server -> .host", () => expect(h).toBe("localhost"));
  test("cfg.server -> .port", () => expect(p).toBe(3000));
  test("cfg?.server?.host via var", () => expect(h2).toBe("localhost"));
});
