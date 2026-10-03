// A listening `net` server keeps the program running, and the deadlock detector
// knows it does.
//
// Neither was true until #2893. `net::registry::source` answered
// `Pending::Blocked` for a bound listener, which by `entry::loops`' contract
// neither holds the program open nor bounds the host's sleep, so:
//
//   $ rts run p3_net.ts   -> "listening", rc=0, immediately; every later
//                            connection refused
//   $ node b_listen.js    -> alive indefinitely (killed at 8s, rc=124)
//
// # The half that makes it more than an early exit
//
// `promise_await` reads the SAME accounting: `pump_sources()` answering `None`
// is how it decides an `await` can never finish. So
// `await new Promise(() => {})` beside a listening server — the ordinary way a
// server program parks — did not merely exit, it THREW
// "await: this promise cannot settle". Measured on `1d070295c`:
//
//   listening
//   rts: uncaught exception (tag 1): await: this promise cannot settle — …
//
// One accounting, two readers. This file pins the reader, which is why it parks
// rather than counting events.
//
// # Why there is no client here, and why the timer is UNREF'd
//
// The server has to be the ONLY thing in the `In` set, or the fixture measures
// whatever else is. A connected socket has answered `Pending::In` since before
// this change, so dialling would hold the program open on its own and prove
// nothing about the listener; a ref'd `setTimeout` is the very workaround being
// removed. So: a listening server, plus an unrefed timer that settles the park.
//
// An unrefed timer is pumped on every pass and contributes no deadline. It can
// therefore only fire if something ELSE keeps the loop turning — and the server
// is the only candidate. If the listener does not hold the program open, nothing
// is outstanding, the park raises, and this file is red in milliseconds. That is
// the discriminator, and it is the whole design of the file.
import { describe, test, expect } from "rts:test";
import { createServer } from "node:net";

const seen: string[] = [];
let finish: (value: string) => void = () => {};
const parked: Promise<string> = new Promise((resolve: any) => {
  finish = resolve;
});

const watchdog: any = setTimeout(() => {
  throw new Error("the listener did not keep the loop turning; saw [" + seen.join(",") + "]");
}, 6000);
watchdog.unref();

const server = createServer((connection: any) => {
  seen.push("connection");
  connection.end();
});

// Port 0 and not a number: a fixed port is a fixture that fails whenever
// anything else on the machine holds it.
server.listen(0, "127.0.0.1", () => {
  seen.push("listening");
  const settle: any = setTimeout(() => {
    seen.push("pumped-while-listening");
    finish("settled");
  }, 60);
  settle.unref();
});

const outcome = await parked;
// Before the assertions: an unclosed listener would leave the `In` set
// non-empty forever, and `rts test` would kill this child at `RTS_TEST_TIMEOUT`
// and report it as a failure. `close()` releasing the program is therefore
// pinned by this file reaching its end at all — the opposite defect to the one
// above and the worse of the two.
clearTimeout(watchdog);
server.close();

describe("a listening net server holds the program open", () => {
  test("the loop keeps turning with only a listener in the ref set", () => {
    expect(outcome).toBe("settled");
    expect(seen.join(",")).toBe("listening,pumped-while-listening");
  });

  test("parking on an unsettleable promise beside a listener does not raise", () => {
    // Reaching this line IS the assertion: the deadlock detector reads
    // `pump_sources`, and had it still answered `None` here the `await` above
    // would have thrown before `describe` was ever evaluated.
    expect(seen.length).toBe(2);
  });
});
