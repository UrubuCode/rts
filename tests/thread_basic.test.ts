import { describe, test, expect } from "rts:test";
import { gc, thread, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Counter compartilhado: a worker incrementa pelo valor que vem como
// argumento. Apos #206 (callconv) + #242 (call_indirect match), o
// arg de thread.spawn(fp, N) chega corretamente ao worker.
const counter = atomic.i64_new(0);

function worker(delta: i64): void {
  atomic.i64_fetch_add(counter, delta);
}

// 1) thread.id da thread atual != 0 e estavel
const id1 = thread.id();
const id2 = thread.id();
if (id1 == 0) {
  print("FAIL: thread.id retornou 0");
} else {
  print("id-ok");
}
if (id1 == id2) {
  print("id-stable");
} else {
  print("FAIL: id-instavel");
}

// 2) sleep_ms smoke (1ms)
thread.sleep_ms(1);
print("sleep-ok");

// 3) spawn(fp, 7) retorna handle != 0 — arg 7 deve chegar ao worker
const fp = getPointer(worker);
const t = thread.spawn(fp, 7);
if (t == 0) {
  print("FAIL: thread.spawn retornou 0");
} else {
  print("spawn-ok");
}

// 4) join consome o handle e nao crasha
thread.join(t);
print("join-ok");

// 5) Apos join, counter deve ter sido incrementado pelo worker em 7
const v = atomic.i64_load(counter);
const hv = gc.string_from_i64(v);
print(hv); gc.string_free(hv); // 7

// 6) detach smoke — spawn + detach nao bloqueiam. Passa 0 pra nao
//    perturbar counter (race com 5 acima ja medido).
const t2 = thread.spawn(fp, 0);
if (t2 == 0) {
  print("FAIL: spawn 2 retornou 0");
} else {
  print("spawn2-ok");
}
thread.detach(t2);
print("detach-ok");

describe("fixture:thread_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "id-ok\nid-stable\nsleep-ok\nspawn-ok\njoin-ok\n7\nspawn2-ok\ndetach-ok\n"
    );
  });
});
