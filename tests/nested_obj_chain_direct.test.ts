import { describe, test, expect } from "rts:test";

// Acesso encadeado direto cfg.server.host (sem var intermediaria)
// e variacoes — em var/print/concat/dentro de classe via this.

const cfg = { server: { host: "localhost", port: 3000 } };

const h = cfg.server.host;
const p = cfg.server.port;

const cfg2 = { a: { b: { c: "deep" } } };
const c = cfg2.a.b.c;

let captured = "";
function print(v: string): void { captured += v + "\n"; }
print(cfg.server.host);

// Concat direto
const concat = "host=" + cfg.server.host;

// Em condicional
let cond = "";
if (cfg.server.host == "localhost") {
  cond = "match";
}

// Numero direto em expressao
const sum = cfg.server.port + 1;

// Em loop
let loopOut = "";
for (let i = 0; i < 2; i++) {
  loopOut += cfg.server.host;
}

// Dentro de fn
function getHost(): string {
  return cfg.server.host;
}
const fnRet = getHost();

// Dentro de classe via const local da classe
class Reader {
  read(): string {
    const local = { srv: { addr: "1.2.3.4" } };
    return local.srv.addr;
  }
}
const readerOut = new Reader().read();

// Mix: cfg.server.host dentro de template-like concat dentro de fn
function build(): string {
  return cfg.server.host + ":" + cfg.server.port;
}
const built = build();

describe("nested_obj_chain_direct", () => {
  test("cfg.server.host direto", () => expect(h).toBe("localhost"));
  test("cfg.server.port direto", () => expect(p).toBe(3000));
  test("triplo aninhado a.b.c", () => expect(c).toBe("deep"));
  test("print(cfg.server.host)", () => expect(captured).toBe("localhost\n"));
  test("concat host=...", () => expect(concat).toBe("host=localhost"));
  test("if condicional cfg.server.host", () => expect(cond).toBe("match"));
  test("cfg.server.port + 1", () => expect(sum).toBe(3001));
  test("loop 2x cfg.server.host", () => expect(loopOut).toBe("localhostlocalhost"));
  test("fn return cfg.server.host", () => expect(fnRet).toBe("localhost"));
  test("classe local.srv.addr", () => expect(readerOut).toBe("1.2.3.4"));
  test("classe build host:port", () => expect(built).toBe("localhost:3000"));
});
