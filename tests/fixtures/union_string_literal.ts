// String literal union inline (sem type alias).
import { io } from "rts";

function settings(m: "fast" | "slow" | "auto"): string {
  if (m == "fast") return "high-perf";
  if (m == "slow") return "low-power";
  return "balanced";
}

io.print(settings("fast"));
io.print(settings("slow"));
io.print(settings("auto"));
