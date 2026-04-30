import { describe, test, expect } from "rts:test";
import { gc, sync } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) Mutex roundtrip: new + lock + read internal value
const m = sync.mutex_new(7);
if (m == 0) {
  print("FAIL: mutex_new retornou 0");
} else {
  print("mutex-ok");
}

const v0 = sync.mutex_lock(m);
const h0 = gc.string_from_i64(v0);
print(h0); gc.string_free(h0); // 7

// 2) mutex_set enquanto temos o lock + unlock
sync.mutex_set(m, 99);
sync.mutex_unlock(m);

// 3) Re-lock para verificar valor persistido
const v1 = sync.mutex_lock(m);
const h1 = gc.string_from_i64(v1);
print(h1); gc.string_free(h1); // 99
sync.mutex_unlock(m);

// 4) try_lock funciona quando livre
const v2 = sync.mutex_try_lock(m);
const h2 = gc.string_from_i64(v2);
print(h2); gc.string_free(h2); // 99
sync.mutex_unlock(m);

sync.mutex_free(m);
print("mutex-freed");

// 5) RwLock: read + write guards
const r = sync.rwlock_new(11);
if (r == 0) {
  print("FAIL: rwlock_new retornou 0");
} else {
  print("rwlock-ok");
}
const rg1 = sync.rwlock_read(r);
const rg2 = sync.rwlock_read(r);
if (rg1 == 0 || rg2 == 0) {
  print("FAIL: read guards");
} else {
  print("read-guards-ok");
}
sync.rwlock_unlock(rg1);
sync.rwlock_unlock(rg2);
const wg = sync.rwlock_write(r);
if (wg == 0) {
  print("FAIL: write guard");
} else {
  print("write-guard-ok");
}
sync.rwlock_unlock(wg);

// 6) OnceLock: only first call_once executes the function
function setFlag(): void {
  __rtsCapturedOutput += "ran-once\n";
}

const o = sync.once_new();
if (o == 0) {
  print("FAIL: once_new retornou 0");
} else {
  print("once-ok");
}
const fp = getPointer(setFlag);
sync.once_call(o, fp);
sync.once_call(o, fp);
sync.once_call(o, fp);
print("after-once");

describe("fixture:sync_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "mutex-ok\n7\n99\n99\nmutex-freed\nrwlock-ok\nread-guards-ok\nwrite-guard-ok\nonce-ok\nran-once\nafter-once\n"
    );
  });
});
