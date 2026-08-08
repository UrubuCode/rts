import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Este ficheiro pinava o comportamento defensivo do namespace `rts`, onde uma
// Promise era um inteiro: `state(0)` = -1, `wait(0)` = 0, `resolve(0, x)` = 0.
// A superficie que fica nao tem handles, logo nao existe handle invalido a
// defender — nao ha assercao equivalente para "0 nao e' Promise". O que essas
// linhas mediam DE FACTO, e que o padrao promete e continua sendo verificado
// aqui, e' o que sobra quando se tira o handle:
//   - `await` sobre nao-thenable devolve o proprio valor (era `wait(456)`=456);
//   - esperar a mesma Promise duas vezes da' o mesmo valor;
//   - o primeiro settle vence e os seguintes sao no-op.
// Cada linha diz qual das antigas substitui.

let st: i64 = 0;
let val: any = 0;
async function settle(p: any): i64 {
  st = 0;
  val = 0;
  await p.then((v: any) => { st = 1; val = v; return v; })
         .catch((e: any) => { st = 2; val = e; return e; });
  return st;
}

// (era state_0 / wait_0 / try_value_0 / resolve_0 / reject_0)
// Sem handles, 0 e' apenas um valor: Promise.resolve(0) e' fulfilled com 0, e
// nao ha resolve/reject a aplicar de fora sobre ele.
await settle(Promise.resolve(0));
print("zero_state=" + st);
print("zero_value=" + val);

// (era wait_garbage=456) `await` sobre nao-thenable = o proprio valor. Esta
// e' literalmente a mesma garantia, agora escrita na superficie padrao.
print("await_non_thenable=" + (await 456));

// (era p_state / p_wait / p_wait_again) Esperar duas vezes e' estavel.
const p = Promise.resolve(42);
await settle(p);
print("p_state=" + st);
print("p_wait=" + (await p));
print("p_wait_again=" + (await p));

// (era re_resolve=0 / r_value=1) Segundo resolve nao substitui o valor.
const r = new Promise((res: any, rej: any) => { res(1); res(99); });
await settle(r);
print("r_state=" + st);
print("r_value=" + val);

// (era re_reject=0 / rj_value=7) Segundo reject nao substitui a razao.
const rj = new Promise((res: any, rej: any) => { rej(7); rej(99); });
await settle(rj);
print("rj_state=" + st);
print("rj_value=" + val);

// (era first=1 / second=0 / third=0 / idem_value=1) resolve, resolve, reject:
// so' o primeiro tem efeito — fulfilled com 1.
const idem = new Promise((res: any, rej: any) => { res(1); res(2); rej(3); });
await settle(idem);
print("idem_state=" + st);
print("idem_value=" + val);

describe("promise invalid handles + edge cases", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "zero_state=1\nzero_value=0\nawait_non_thenable=456\np_state=1\np_wait=42\np_wait_again=42\nr_state=1\nr_value=1\nrj_state=2\nrj_value=7\nidem_state=1\nidem_value=1\n"
    );
  });
});
