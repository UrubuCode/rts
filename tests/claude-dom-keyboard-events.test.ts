import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

const doc = parseDocument(
  "<body><div id='pai'><button id='b'>x</button></div></body>",
);
const button = doc.getElementById("b");
const parent = doc.getElementById("pai");

let downKey = "";
let downCode = "";
let downTarget = "";
let downCurrent = "";
let downCtrl = false;
let downRepeat = true;
let upKey = "";
let upType = "";
let parentEvents = 0;
let documentEvents = 0;

if (button !== null) {
  button.addEventListener("keydown", (event: any) => {
    downKey = event.key;
    downCode = event.code;
    downTarget = event.target.tagName;
    downCurrent = event.currentTarget.tagName;
    downCtrl = event.ctrlKey;
    downRepeat = event.repeat;
  });
  button.addEventListener("keyup", (event: any) => {
    upKey = event.key;
    upType = event.type;
  });
}
if (parent !== null) {
  parent.addEventListener("keydown", (_event: any) => {
    parentEvents = parentEvents + 1;
  });
}
doc.addEventListener("keydown", (_event: any) => {
  documentEvents = documentEvents + 1;
});

if (button !== null) {
  dom.focusInput(doc._dom, button.nodeId);
  // flags: pressed=1, repeat=2, ctrl=4, shift=8, alt=16, meta=32.
  dom.pushRawKeyboardEvent(doc._dom, 112, 1 + 4);
}
const downPumped = pumpKeyboardEvents(doc);

if (button !== null) {
  dom.pushRawKeyboardEvent(doc._dom, 112, 0);
}
const upPumped = pumpKeyboardEvents(doc);

describe("emissor de eventos de teclado", () => {
  test("emite keydown com metadados e bubbling", () => {
    expect(downPumped).toBe(1);
    expect(downKey).toBe("m");
    expect(downCode).toBe("KeyM");
    expect(downTarget).toBe("BUTTON");
    expect(downCurrent).toBe("BUTTON");
    expect(downCtrl).toBe(true);
    expect(downRepeat).toBe(false);
    expect(parentEvents).toBe(1);
    expect(documentEvents).toBe(1);
  });

  test("emite keyup no mesmo target", () => {
    expect(upPumped).toBe(1);
    expect(upKey).toBe("m");
    expect(upType).toBe("keyup");
  });
});
