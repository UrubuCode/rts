import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// String enum como discriminante de switch (via if-else).

enum Action {
  Save = "save",
  Load = "load",
  Quit = "quit",
}

function handle(a: string): string {
  if (a == Action.Save) return "salvando...";
  if (a == Action.Load) return "carregando...";
  if (a == Action.Quit) return "tchau";
  return "?";
}

const acts: string[] = [];
acts.push(Action.Save);
acts.push(Action.Load);
acts.push(Action.Quit);

for (const a of acts) {
  print(handle(a));
}

describe("fixture:enum_string_switch", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("salvando...\ncarregando...\ntchau\n");
  });
});
