// A host module's default import and its `require` value are both its
// `module.exports`, whatever TYPE that is.
//
// Node has ONE rule here: the ESM default of a CommonJS module is
// `module.exports`. Four `node:` modules export a FUNCTION (`events` →
// `EventEmitter`, `assert` → `ok`, `stream` → `Stream`, `module` → `Module`),
// and the rest export an object of properties. Measured on Node 22.23.2,
// 2026-10-02.
//
// This file pins both types in both directions, because the engine held two
// answers to one question: `require` read a declared CommonJS value while a
// default import read the namespace object, so `import EventEmitter from
// "events"` answered something that is not a constructor. The `fs` cases are
// the non-regression half — an object-exporting module's default must stay the
// namespace.
import { describe, test, expect } from "rts:test";

import EventEmitter from "events";
import NodeEventEmitter from "node:events";
import fs from "node:fs";
import * as eventsNamespace from "events";
import WebSocketDefault from "ws";

// The `require` side comes from a second module rather than from here — that
// file's header says why the five CommonJS names are not in scope for a suite
// file that imports nothing.
import {
  requiredEvents,
  requiredNodeEvents,
  requiredFs,
  requiredWs,
} from "./_claude_host_module_exports_cjs";

describe("a host module whose module.exports is a function", () => {
  test("default import of events is the EventEmitter constructor", () => {
    expect(typeof EventEmitter).toBe("function");
    expect(new EventEmitter() instanceof EventEmitter).toBe(true);
  });

  test("node: and bare spelling bind the same constructor", () => {
    expect(NodeEventEmitter).toBe(EventEmitter);
  });

  test("the constructor carries itself as .EventEmitter, as Node does", () => {
    expect(EventEmitter.EventEmitter).toBe(EventEmitter);
  });

  test("the default import answers what the emitter protocol needs", () => {
    const emitter = new EventEmitter();
    let seen = "";
    emitter.on("ping", (value: string) => { seen = value; });
    emitter.emit("ping", "hit");
    expect(seen).toBe("hit");
  });

  test("require of events is that same constructor", () => {
    expect(requiredEvents).toBe(EventEmitter);
    expect(requiredNodeEvents).toBe(EventEmitter);
  });

  test("the namespace still exposes the named export", () => {
    expect(typeof eventsNamespace.EventEmitter).toBe("function");
  });

  test("import * as ns exposes ns.default as module.exports", () => {
    expect(eventsNamespace.default).toBe(EventEmitter);
  });
});

describe("a host module whose module.exports is an object", () => {
  test("default import of node:fs is the namespace", () => {
    expect(typeof fs).toBe("object");
    expect(typeof fs.readFileSync).toBe("function");
  });

  test("require of node:fs answers that same object surface", () => {
    expect(typeof requiredFs).toBe("object");
    expect(typeof requiredFs.readFileSync).toBe("function");
  });
});

describe("ws, whose module.exports is the WebSocket class", () => {
  test("default import and require answer the same constructor", () => {
    expect(typeof WebSocketDefault).toBe("function");
    expect(requiredWs).toBe(WebSocketDefault);
  });

  test("the server constructor hangs off it under both names", () => {
    expect(typeof requiredWs.Server).toBe("function");
    expect(requiredWs.WebSocketServer).toBe(requiredWs.Server);
  });

  test("the readyState constants are statics of that class", () => {
    expect(requiredWs.CONNECTING).toBe(0);
    expect(requiredWs.OPEN).toBe(1);
    expect(requiredWs.CLOSING).toBe(2);
    expect(requiredWs.CLOSED).toBe(3);
  });
});
