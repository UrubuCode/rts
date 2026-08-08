import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let counter = 0;
function onTick(): void {
  counter += 1;
  print(`tick:${counter}`);
}

// Os mesmos passos sobre o `EventEmitter` do `node:events`: `emitter_new` vira
// `new`, `listener_count` vira `listenerCount`, `emit0` vira `emit` sem
// argumentos, e `emitter_free` some (o emitter é coletado, não tem handle).
const e = new EventEmitter();
print(`count0=${e.listenerCount("tick")}`);

e.on("tick", onTick);
print(`count1=${e.listenerCount("tick")}`);

e.emit("tick");
e.emit("tick");

print(`final_counter=${counter}`);

e.removeAllListeners("tick");
print(`after_remove=${e.listenerCount("tick")}`);

describe("events_namespace", () => {
  test("emit dispatches and listener_count tracks", () => {
    expect(__rtsCapturedOutput).toBe(
      "count0=0\ncount1=1\ntick:1\ntick:2\nfinal_counter=2\nafter_remove=0\n"
    );
  });
});
