import { describe, test, expect } from "rts:test";
import { gc, math } from "rts";

// Cobre as APIs runtime adicionadas em #147 passo 4: alloc nativo de
// instancia, leitura de class tag, roundtrip i64/i32/f64, e validacao
// defensiva de handle invalido.

describe("gc.instance basics", () => {
  test("instance_new aloca e instance_class le tag", () => {
    const tag = gc.string_from_static("MyClass");
    const h = gc.instance_new(32, tag);
    expect(h != 0).toBe(true);
    expect(gc.instance_class(h)).toBe(tag);
    gc.instance_free(h);
    gc.string_free(tag);
  });

  test("store/load i64 roundtrip", () => {
    const h = gc.instance_new(32, 0);
    expect(gc.instance_store_i64(h, 8, 1234567890)).toBe(1);
    expect(gc.instance_load_i64(h, 8)).toBe(1234567890);
    gc.instance_free(h);
  });

  test("store/load i32 roundtrip", () => {
    const h = gc.instance_new(16, 0);
    expect(gc.instance_store_i32(h, 0, 42)).toBe(1);
    expect(gc.instance_load_i32(h, 0)).toBe(42);
    gc.instance_free(h);
  });

  test("store/load f64 roundtrip", () => {
    const h = gc.instance_new(16, 0);
    expect(gc.instance_store_f64(h, 0, math.PI)).toBe(1);
    const v = gc.instance_load_f64(h, 0);
    // Round-trip exato em f64 little-endian.
    expect(v).toBe(math.PI);
    gc.instance_free(h);
  });

  test("handle invalido retorna 0", () => {
    expect(gc.instance_class(0)).toBe(0);
    expect(gc.instance_load_i64(0, 0)).toBe(0);
    expect(gc.instance_store_i64(0, 0, 1)).toBe(0);
    expect(gc.instance_free(0)).toBe(0);
  });
});
