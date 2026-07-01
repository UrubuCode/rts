import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Operacoes em handle 0 — comportamento defensivo.
print("state_0=" + promise.state(0));         // -1
print("wait_0=" + promise.wait(0));           // 0
print("try_value_0=" + promise.try_value(0)); // 0
print("resolve_0=" + promise.resolve(0, 99)); // 0 (no-op)
print("reject_0=" + promise.reject(0, 1));    // 0

// 2. Handle aleatorio invalido (gen 0).
print("state_garbage=" + promise.state(123));    // -1
// `wait` de um valor que nao e' Promise viva = o proprio valor (semantica JS de
// `await` sobre nao-thenable; async fns rodam sincrono no motor novo).
print("wait_garbage=" + promise.wait(456));      // 456

// 3. Handle valido apos free conceitual — wait em Promise nao reaproveitavel.
const p = promise.new_resolved(42);
print("p_state=" + promise.state(p));
print("p_wait=" + promise.wait(p));
// Wait de novo e' OK (multiplas chamadas no mesmo handle resolvido).
print("p_wait_again=" + promise.wait(p));

// 4. resolve em Promise ja' resolved retorna 0.
const r = promise.new_resolved(1);
print("re_resolve=" + promise.resolve(r, 99));  // 0
print("r_value=" + promise.try_value(r));       // 1 (valor original)

// 5. reject em Promise ja' rejected retorna 0.
const rj = promise.new_rejected(7);
print("re_reject=" + promise.reject(rj, 99));   // 0
print("rj_value=" + promise.try_value(rj));     // 7 (valor original)

// 6. Estados intermediarios — resolve seguido de outro pra confirmar idempotencia.
const idem = promise.new_pending();
print("first=" + promise.resolve(idem, 1));     // 1
print("second=" + promise.resolve(idem, 2));    // 0 (no-op)
print("third=" + promise.reject(idem, 3));      // 0 (no-op)
print("idem_value=" + promise.wait(idem));     // 1

describe("promise invalid handles + edge cases", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "state_0=-1\nwait_0=0\ntry_value_0=0\nresolve_0=0\nreject_0=0\nstate_garbage=-1\nwait_garbage=456\np_state=1\np_wait=42\np_wait_again=42\nre_resolve=0\nr_value=1\nre_reject=0\nrj_value=7\nfirst=1\nsecond=0\nthird=0\nidem_value=1\n"
    );
  });
});
