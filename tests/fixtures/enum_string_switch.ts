// String enum como discriminante de switch (via if-else).
import { io } from "rts";

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
  io.print(handle(a));
}
