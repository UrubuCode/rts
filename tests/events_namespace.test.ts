import { describe, test, expect } from "rts:test";
import { events } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let counter = 0;
function onTick(): void {
  counter += 1;
  print(`tick:${counter}`);
}

const e = events.emitter_new();
print(`count0=${events.listener_count(e, "tick")}`);

events.on(e, "tick", onTick);
print(`count1=${events.listener_count(e, "tick")}`);

events.emit0(e, "tick");
events.emit0(e, "tick");

print(`final_counter=${counter}`);

events.remove_all_listeners(e, "tick");
print(`after_remove=${events.listener_count(e, "tick")}`);

events.emitter_free(e);

describe("events_namespace", () => {
  test("emit dispatches and listener_count tracks", () => {
    expect(__rtsCapturedOutput).toBe(
      "count0=0\ncount1=1\ntick:1\ntick:2\nfinal_counter=2\nafter_remove=0\n"
    );
  });
});
