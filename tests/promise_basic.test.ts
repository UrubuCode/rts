import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// O namespace `rts` expunha Promise como handle inteiro: `state()` (0|1|2|-1),
// `wait()` bloqueante, `try_value()`, `new_pending()` e `resolve()/reject()`
// devolvendo 1 ou 0 conforme tivessem efeito. A superficie padrao nao tem
// handle nem estado sincrono nem espera bloqueante; o que ela GARANTE e'
// observavel por `.then`/`.catch`. `settle` abaixo faz exatamente essa leitura
// e produz o mesmo par (estado, valor) que as assercoes originais comparavam.
let st: i64 = 0;
let val: any = 0;
async function settle(p: any): i64 {
  st = 0;
  val = 0;
  await p.then((v: any) => { st = 1; val = v; return v; })
         .catch((e: any) => { st = 2; val = e; return e; });
  return st;
}

// Promise.resolve — nasce ja' fulfilled.
const p1 = Promise.resolve(42);
await settle(p1);
print("state=" + st);   // 1 (fulfilled)
print("wait=" + val);   // 42

// Promise.reject.
const p2 = Promise.reject(7);
await settle(p2);
print("state=" + st);       // 2 (rejected)
print("try_value=" + val);  // 7

// Idempotencia: o segundo resolve e' no-op. O padrao nao devolve 1|0 do
// resolve — o que ele promete e' que o PRIMEIRO settle vence, e e' isso que a
// assercao original media com "primeiro_resolve=1 / segundo_resolve=0". Aqui
// isso e' lido no valor final, que so' pode ser 1 se o segundo foi ignorado.
const p3 = new Promise((res: any, rej: any) => { res(1); res(2); });
await settle(p3);
print("valor_final=" + val);   // 1

// reject apos resolve tambem e' no-op (ja' settled).
const p4 = new Promise((res: any, rej: any) => { res(100); rej(999); });
await settle(p4);
print("estado_p4=" + st);   // 1 — continua fulfilled, o reject nao pegou
print("valor_p4=" + val);   // 100

// `promise.state(0)` devolvia -1 porque 0 nao era handle de Promise nenhuma.
// Handles nao existem na superficie que fica, entao nao ha estado invalido a
// observar; o que substitui essa garantia e' `Promise.resolve` sobre um
// nao-thenable, que produz um fulfilled com o proprio valor.
await settle(Promise.resolve(0));
print("state_nao_thenable=" + st + " valor=" + val);   // 1 0

describe("promise primitive (issue #412)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "state=1\nwait=42\nstate=2\ntry_value=7\nvalor_final=1\nestado_p4=1\nvalor_p4=100\nstate_nao_thenable=1 valor=0\n"
    );
  });
});
