import { describe, test, expect } from "rts:test";

let out = "";

// (#210) Nested object destructuring com tipos preservados.
const cfg = { db: { host: "localhost", port: 5432 } };
const { db: { host, port } } = cfg;
out += host + ":" + port + "\n";

// Alias + nested
const cfg2 = { server: { name: "rts", workers: 4 } };
const { server: srv } = cfg2;
out += srv.name + " has " + srv.workers + " workers\n";

// Triplo nested
const deep = { a: { b: { c: 42 } } };
const { a: { b: { c } } } = deep;
out += c + "\n";

describe("destruct_nested_object", () => {
  test("nested destructuring preserva tipos via local_nested_obj_field_types", () =>
    expect(out).toBe("localhost:5432\nrts has 4 workers\n42\n"));
});
