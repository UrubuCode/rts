import { describe, test, expect } from "rts:test";

const doc = parseDocument("<body><div id='pai'><button id='b'>x</button></div></body>");
const button = doc.getElementById("b");
const parent = doc.getElementById("pai");
const order: string[] = [];
let onceCount = 0;
let passiveDefaultPrevented = false;
let activeDefaultPrevented = false;
const noBubbleOrder: string[] = [];

if (parent !== null) {
  parent.addEventListener("order", (event: any) => {
    order.push("parent-capture-" + event.eventPhase);
  }, { capture: true });
  parent.addEventListener("order", (event: any) => {
    order.push("parent-bubble-" + event.eventPhase);
  });
}
if (button !== null) {
  button.addEventListener("order", (event: any) => {
    order.push("target-capture-" + event.eventPhase);
  }, { capture: true });
  button.addEventListener("order", (event: any) => {
    order.push("target-bubble-" + event.eventPhase);
  });
  button.addEventListener("once", (_event: any) => {
    onceCount = onceCount + 1;
  }, { once: true });
  button.addEventListener("cancel", (event: any) => {
    event.preventDefault();
    passiveDefaultPrevented = event.defaultPrevented;
  }, { passive: true });
  button.addEventListener("cancel", (event: any) => {
    event.preventDefault();
    activeDefaultPrevented = event.defaultPrevented;
  });
  button.dispatchEvent("order");
  button.dispatchEvent("once");
  button.dispatchEvent("once");
  button.dispatchEvent("cancel");
  if (parent !== null) {
    parent.addEventListener("no-bubble", (_event: any) => noBubbleOrder.push("parent-capture"), { capture: true });
    parent.addEventListener("no-bubble", (_event: any) => noBubbleOrder.push("parent-bubble"));
  }
  button.addEventListener("no-bubble", (_event: any) => noBubbleOrder.push("target-capture"), { capture: true });
  button.addEventListener("no-bubble", (_event: any) => noBubbleOrder.push("target-bubble"));
  button.dispatchEvent(new Event("no-bubble", { bubbles: false }));
}

const removedDoc = parseDocument("<body><button id='b'>x</button></body>");
const removedButton = removedDoc.getElementById("b");
let removedCount = 0;
const removedHandler = (_event: any) => { removedCount = removedCount + 1; };
if (removedButton !== null) {
  removedButton.addEventListener("remove", removedHandler, { capture: true });
  removedButton.removeEventListener("remove", removedHandler, { capture: true });
  removedButton.dispatchEvent("remove");
}

describe("opções de listeners DOM", () => {
  test("executa capture, target e bubble com eventPhase correcto", () => {
    expect(order[0]).toBe("parent-capture-1");
    expect(order[1]).toBe("target-capture-2");
    expect(order[2]).toBe("target-bubble-2");
    expect(order[3]).toBe("parent-bubble-3");
  });

  test("once remove o callback antes do segundo dispatch", () => {
    expect(onceCount).toBe(1);
  });

  test("passive não permite preventDefault, listener normal permite", () => {
    expect(passiveDefaultPrevented).toBe(false);
    expect(activeDefaultPrevented).toBe(true);
  });

  test("removeEventListener respeita callback e capture", () => {
    expect(removedCount).toBe(0);
  });

  test("bubbles false mantém capture e bloqueia bubbling", () => {
    expect(noBubbleOrder[0]).toBe("parent-capture");
    expect(noBubbleOrder[1]).toBe("target-capture");
    expect(noBubbleOrder[2]).toBe("target-bubble");
    expect(noBubbleOrder.length).toBe(3);
  });
});
