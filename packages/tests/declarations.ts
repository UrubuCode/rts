import { io } from "rts";

export function testDeclarations(): void {
  const base: number = 5;
  let incremented: number = base + 2;
  var scaled: number = incremented * 3;
  const fallback = undefined ?? "fallback-value";
  const text = ("" || "ok") + "/" + (0 || 42);
  const strict = (scaled === 21) && (incremented == 7);
  const negated = !false;

  io.print("[tests/declarations] scaled=" + scaled);
  io.print("[tests/declarations] fallback=" + fallback);
  io.print("[tests/declarations] text=" + text);
  io.print("[tests/declarations] strict=" + strict);
  io.print("[tests/declarations] negated=" + negated);
}
