import { describe, test, expect } from "rts:test";
import { gc, sync } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #280: antes era UB free(m) com lock ativo (Box dropped, MutexGuard
// 'static dangling). Agora Arc + clone-on-lock ancora o Mutex enquanto
// o guard existir.

const m = sync.mutex_new(7);
const v = sync.mutex_lock(m);
sync.mutex_set(m, 42);

// Liberacao do handle enquanto trancado: antes UB, agora seguro
// (o Arc mantido pelo guard mantem o Mutex vivo).
sync.mutex_free(m);

// Apos free com lock ativo, mutex_free libera o handle e remove o
// guard do mapa thread-local — sequencia limpa, sem crash.

const ok = gc.string_from_static("survived-free-before-unlock");
print(ok); gc.string_free(ok);

const initial = gc.string_from_i64(v);
print(initial); gc.string_free(initial);

// Caso normal: lock/unlock/free na ordem correta continua funcionando.
const m2 = sync.mutex_new(100);
sync.mutex_lock(m2);
sync.mutex_set(m2, 200);
sync.mutex_unlock(m2);
const v2 = sync.mutex_lock(m2);
sync.mutex_unlock(m2);
sync.mutex_free(m2);

const after = gc.string_from_i64(v2);
print(after); gc.string_free(after);

describe("fixture:sync_mutex_free_before_unlock", () => {
  test("free before unlock no longer UB; normal flow still works", () => {
    expect(__rtsCapturedOutput).toBe("survived-free-before-unlock\n7\n200\n");
  });
});
