import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// String literal union como discriminante de estado.

function progress(state: "init" | "running" | "done"): string {
  if (state == "init") return "preparando";
  if (state == "running") return "em execucao";
  return "finalizado";
}

const states: string[] = [];
states.push("init");
states.push("running");
states.push("done");

for (const s of states) {
  print(progress(s));
}

describe("fixture:union_status_codes", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("preparando\nem execucao\nfinalizado\n");
  });
});
