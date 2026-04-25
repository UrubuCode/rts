// String literal union como discriminante de estado.
import { io } from "rts";

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
  io.print(progress(s));
}
