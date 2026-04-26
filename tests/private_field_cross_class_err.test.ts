import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_PRIVATE_FIELD_CROSS_CLASS_ERR = "// ERRO esperado: A não declara #x mas tenta acessar de instância de B.\nclass A {\n    foo(b: B): number { return b.#x; }\n}\nclass B {\n    #x: number = 5;\n}\nnew A().foo(new B());\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:private_field_cross_class_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_private_field_cross_class_err.ts", SOURCE_PRIVATE_FIELD_CROSS_CLASS_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_private_field_cross_class_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
