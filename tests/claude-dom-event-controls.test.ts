import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

const doc = parseDocument("<body><div id='pai'><button id='b'>x</button></div></body>");
const button = doc.getElementById("b");
const parent = doc.getElementById("pai");

let sameTarget = 0;
let parentEvents = 0;
let defaultWasPrevented = false;

if (button !== null) {
  button.addEventListener("keydown", (event: any) => {
    sameTarget = sameTarget + 1;
    event.stopPropagation();
  });
  button.addEventListener("keydown", (event: any) => {
    sameTarget = sameTarget + 1;
    event.preventDefault();
  });
  button.addEventListener("keydown", (event: any) => {
    sameTarget = sameTarget + 1;
    defaultWasPrevented = event.defaultPrevented;
  });
}
if (parent !== null) {
  parent.addEventListener("keydown", (_event: any) => {
    parentEvents = parentEvents + 1;
  });
}

if (button !== null) {
  dom.focusInput(doc._dom, button.nodeId);
  dom.pushRawKeyboardEvent(doc._dom, 112, 1);
}
const stoppedPumped = pumpKeyboardEvents(doc);

const immediateDoc = parseDocument("<body><div id='pai'><button id='b'>x</button></div></body>");
const immediateButton = immediateDoc.getElementById("b");
const immediateParent = immediateDoc.getElementById("pai");
let immediateFirst = 0;
let immediateSecond = 0;
let immediateParentEvents = 0;
if (immediateButton !== null) {
  immediateButton.addEventListener("keydown", (event: any) => {
    immediateFirst = immediateFirst + 1;
    event.stopImmediatePropagation();
  });
  immediateButton.addEventListener("keydown", (_event: any) => {
    immediateSecond = immediateSecond + 1;
  });
  dom.focusInput(immediateDoc._dom, immediateButton.nodeId);
  dom.pushRawKeyboardEvent(immediateDoc._dom, 112, 1);
}
if (immediateParent !== null) {
  immediateParent.addEventListener("keydown", (_event: any) => {
    immediateParentEvents = immediateParentEvents + 1;
  });
}
const immediatePumped = pumpKeyboardEvents(immediateDoc);

const genericDoc = parseDocument("<body><div id='pai'><button id='b'>x</button></div></body>");
const genericButton = genericDoc.getElementById("b");
const genericParent = genericDoc.getElementById("pai");
let genericTarget = 0;
let genericParentEvents = 0;
let genericDefaultPrevented = false;

const focusDoc = parseDocument("<body><input id='a'><input id='b'></body>");
const focusA = focusDoc.getElementById("a");
const focusB = focusDoc.getElementById("b");
const focusEvents: string[] = [];
if (focusA !== null) {
  focusA.addEventListener("focusin", (_event: any) => { focusEvents.push("a:focusin"); });
  focusA.addEventListener("focus", (_event: any) => { focusEvents.push("a:focus"); });
  focusA.addEventListener("focusout", (_event: any) => { focusEvents.push("a:focusout"); });
  focusA.addEventListener("blur", (_event: any) => { focusEvents.push("a:blur"); });
}
if (focusB !== null) {
  focusB.addEventListener("focusin", (_event: any) => { focusEvents.push("b:focusin"); });
  focusB.addEventListener("focus", (_event: any) => { focusEvents.push("b:focus"); });
}
if (focusA !== null && focusB !== null) {
  dom.focusInput(focusDoc._dom, focusA.nodeId);
  pumpEventCallbacks(focusDoc);
  dom.focusInput(focusDoc._dom, focusB.nodeId);
  pumpEventCallbacks(focusDoc);
}
if (genericButton !== null) {
  genericButton.addEventListener("click", (event: any) => {
    genericTarget = genericTarget + 1;
    event.preventDefault();
  });
  genericButton.addEventListener("click", (event: any) => {
    genericDefaultPrevented = event.defaultPrevented;
    event.stopPropagation();
  });
  genericButton.dispatchEvent("click");
}
if (genericParent !== null) {
  genericParent.addEventListener("click", (_event: any) => {
    genericParentEvents = genericParentEvents + 1;
  });
}

describe("controlo de propagação e cancelamento", () => {
  test("stopPropagation mantém listeners do target e impede bubbling", () => {
    expect(stoppedPumped).toBe(1);
    expect(sameTarget).toBe(3);
    expect(parentEvents).toBe(0);
    expect(defaultWasPrevented).toBe(true);
  });

  test("stopImmediatePropagation impede listeners seguintes e bubbling", () => {
    expect(immediatePumped).toBe(1);
    expect(immediateFirst).toBe(1);
    expect(immediateSecond).toBe(0);
    expect(immediateParentEvents).toBe(0);
  });

  test("focus e blur são emitidos na ordem correcta", () => {
    expect(focusEvents[0]).toBe("a:focusin");
    expect(focusEvents[1]).toBe("a:focus");
    expect(focusEvents[2]).toBe("a:focusout");
    expect(focusEvents[3]).toBe("a:blur");
    expect(focusEvents[4]).toBe("b:focusin");
    expect(focusEvents[5]).toBe("b:focus");
  });

  test("preventDefault fica observável no dispatcher genérico", () => {
    expect(genericTarget).toBe(1);
    expect(genericDefaultPrevented).toBe(true);
    expect(genericParentEvents).toBe(0);
  });
});
