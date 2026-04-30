import { describe, test, expect } from "rts:test";
import { gc, buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Loop de alloc + free nao deve crescer count alem de uma janela
// pequena (handles sao reusados via free_list).
const before = gc.live_count();
for (let i = 0; i < 50; i = i + 1) {
  const buf = buffer.alloc(64);
  buffer.free(buf);
}
const after = gc.live_count();
const grew = after - before;
print(`grew_ok=${grew < 20}`);

describe("handle_cleanup", () => {
  test("free reuses slots", () => expect(__rtsCapturedOutput).toBe("grew_ok=true\n"));
});
