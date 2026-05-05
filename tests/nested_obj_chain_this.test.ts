import { describe, test, expect } from "rts:test";

// (#nested-this) this.cfg.server.host em metodos de classe — depende
// de class meta registrar nested types via scan do constructor.

class Holder {
  cfg: { server: { host: string; port: number } };
  constructor() {
    this.cfg = { server: { host: "h-class", port: 9000 } };
  }
  getHost(): string {
    return this.cfg.server.host;
  }
  getPort(): number {
    return this.cfg.server.port;
  }
  getServer(): string {
    // this.cfg.server retorna handle do submap; concat coage para
    // string handle representando o map (debug-ish). Aqui so' garante
    // que nao trava e que o caminho 1-nivel chained funciona.
    const s: string = this.cfg.server.host;
    return s;
  }
  build(): string {
    return this.cfg.server.host + ":" + this.cfg.server.port;
  }
}

const h = new Holder();
const host = h.getHost();
const port = h.getPort();
const built = h.build();
const srv = h.getServer();

// Hierarquia: subclasse herda field nested do super.
class Sub extends Holder {
  hostUpper(): string {
    return this.cfg.server.host;
  }
}
const sub = new Sub();
const subHost = sub.hostUpper();

// Este é com initializer de propriedade (sem ctor explicito) +
// atribuicao no construtor.
class Cfg {
  data: { conn: { url: string; port: number } };
  constructor() {
    this.data = { conn: { url: "u", port: 1 } };
  }
  url(): string { return this.data.conn.url; }
  port(): number { return this.data.conn.port; }
}
const c = new Cfg();
const cUrl = c.url();
const cPort = c.port();

describe("nested_obj_chain_this", () => {
  test("this.cfg.server.host", () => expect(host).toBe("h-class"));
  test("this.cfg.server.port", () => expect(port).toBe(9000));
  test("classe build host:port", () => expect(built).toBe("h-class:9000"));
  test("classe getServer", () => expect(srv).toBe("h-class"));
  test("subclasse herda nested", () => expect(subHost).toBe("h-class"));
  test("Cfg.data.conn.url", () => expect(cUrl).toBe("u"));
  test("Cfg.data.conn.port", () => expect(cPort).toBe(1));
});
