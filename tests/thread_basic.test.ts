import { describe, test, expect } from "rts:test";
import { gc, thread, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Counter compartilhado: a worker incrementa quando roda. Como o arg
// passado pra spawn nao chega de forma confiavel ao user fn (CallConv
// mismatch entre Tail dos user fns e extern "C" da spawn), o worker
// trabalha apenas via globais. spawn/join validam o lifecycle do handle.
const counter = atomic.i64_new(0);

function worker(): void {
  atomic.i64_fetch_add(counter, 1);
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

// 3) spawn retorna handle != 0
const fp = worker as unknown as number;
const t = thread.spawn(fp, 0);
if (t == 0) {
  print("FAIL: thread.spawn retornou 0");
} else {
  print("spawn-ok");
}

// 4) join consome o handle e nao crasha
thread.join(t);
print("join-ok");

// 5) Apos join, counter deve ter sido incrementado pelo worker
const v = atomic.i64_load(counter);
const hv = gc.string_from_i64(v);
print(hv); gc.string_free(hv); // 1

// 6) detach smoke — spawn + detach nao bloqueiam
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
      "id-ok\nid-stable\nsleep-ok\nspawn-ok\njoin-ok\n1\nspawn2-ok\ndetach-ok\n"
    );
  });
});
