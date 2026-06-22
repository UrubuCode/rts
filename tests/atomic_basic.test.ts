import { describe, test, expect } from "rts:test";
import { atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) AtomicI64 new + load + store
const a = atomic.i64_new(10);
if (a == 0) {
  print("FAIL: i64_new retornou 0");
} else {
  print("i64-ok");
}
const v0 = atomic.i64_load(a);
print(`${v0}`); // 10

atomic.i64_store(a, 42);
const v1 = atomic.i64_load(a);
print(`${v1}`); // 42

// 2) fetch_add / fetch_sub
const prevAdd = atomic.i64_fetch_add(a, 8);
print(`${prevAdd}`); // 42 (valor anterior)
const afterAdd = atomic.i64_load(a);
print(`${afterAdd}`); // 50

const prevSub = atomic.i64_fetch_sub(a, 5);
print(`${prevSub}`); // 50
const afterSub = atomic.i64_load(a);
print(`${afterSub}`); // 45

// 3) CAS sucesso (atual = 45, expected = 45 -> escreve 100)
const casOk = atomic.i64_cas(a, 45, 100);
print(`${casOk}`); // 45 (valor anterior)
const afterCas = atomic.i64_load(a);
print(`${afterCas}`); // 100

// 4) CAS falha (atual = 100, expected = 0 -> nao escreve)
const casFail = atomic.i64_cas(a, 0, 999);
print(`${casFail}`); // 100 (valor atual, igual ao anterior)
const afterCasFail = atomic.i64_load(a);
print(`${afterCasFail}`); // 100 (inalterado)

// 5) AtomicBool roundtrip
const b = atomic.bool_new(false);
if (b == 0) {
  print("FAIL: bool_new retornou 0");
} else {
  print("bool-ok");
}
const b0 = atomic.bool_load(b);
if (b0) { print("b0=true"); } else { print("b0=false"); }
atomic.bool_store(b, true);
const b1 = atomic.bool_load(b);
if (b1) { print("b1=true"); } else { print("b1=false"); }
const swapped = atomic.bool_swap(b, false);
if (swapped) { print("swapped=true"); } else { print("swapped=false"); }
const b2 = atomic.bool_load(b);
if (b2) { print("b2=true"); } else { print("b2=false"); }

// 6) Fences (smoke — sem retorno, so chamar)
atomic.fence_acquire();
atomic.fence_release();
atomic.fence_seq_cst();
print("fences-ok");

describe("fixture:atomic_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "i64-ok\n10\n42\n42\n50\n50\n45\n45\n100\n100\n100\nbool-ok\nb0=false\nb1=true\nswapped=true\nb2=false\nfences-ok\n"
    );
  });
});
